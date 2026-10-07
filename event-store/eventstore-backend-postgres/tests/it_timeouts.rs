//! Pool size and session timeouts reach Postgres, and a stuck lock or
//! statement surfaces as UNAVAILABLE within the configured bound instead of
//! hanging (#368, #370).
mod common;

use std::time::{Duration, Instant};

use eventstore_backend_postgres::{PostgresConfig, PostgresStore};
use eventstore_core::{proto, EventStore, StoreError};
use sqlx::Connection;

/// Same key the store's append-order lock uses: (namespace, hashtext(tenant)).
const APPEND_ORDER_LOCK_NAMESPACE: i32 = 0x0E5_1545;

fn unique(prefix: &str) -> String {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{prefix}-{n}")
}

fn append_req(tenant: &str, aggregate: &str) -> proto::AppendRequest {
    proto::AppendRequest {
        tenant_id: tenant.into(),
        aggregate_id: aggregate.into(),
        aggregate_type: "Timeout".into(),
        expected_aggregate_nonce: 0,
        idempotency_key: String::new(),
        events: vec![proto::EventData {
            meta: Some(proto::EventMetadata {
                event_id: unique("ev"),
                aggregate_nonce: 1,
                event_type: "Happened".into(),
                event_version: 1,
                ..Default::default()
            }),
            payload: b"{}".to_vec(),
        }],
    }
}

async fn show(store: &PostgresStore, setting: &str) -> String {
    sqlx::query_scalar(&format!("SHOW {setting}"))
        .fetch_one(store.pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn pool_size_and_session_settings_are_applied() {
    let url = common::get_test_database_url().await;
    let cfg = PostgresConfig {
        max_connections: 3,
        min_connections: 1,
        statement_timeout: Some(Duration::from_millis(1500)),
        lock_timeout: Some(Duration::from_millis(700)),
        idle_in_transaction_timeout: Some(Duration::from_secs(4)),
        tcp_keepalive: Some(Duration::from_secs(21)),
        ..Default::default()
    };
    let store = PostgresStore::connect_with_config(&url, &cfg)
        .await
        .unwrap();
    assert_eq!(store.pool().options().get_max_connections(), 3);
    assert_eq!(store.pool().options().get_min_connections(), 1);
    assert_eq!(show(&store, "statement_timeout").await, "1500ms");
    assert_eq!(show(&store, "lock_timeout").await, "700ms");
    assert_eq!(
        show(&store, "idle_in_transaction_session_timeout").await,
        "4s"
    );
    // Testcontainers / dev infra connect over TCP, where keepalives apply.
    assert_eq!(show(&store, "tcp_keepalives_idle").await, "21");
    assert_eq!(show(&store, "tcp_keepalives_interval").await, "7");
    assert_eq!(show(&store, "tcp_keepalives_count").await, "3");
}

#[tokio::test]
async fn migrations_are_not_subject_to_statement_timeout() {
    let url = common::get_test_database_url().await;
    let cfg = PostgresConfig {
        statement_timeout: Some(Duration::from_millis(1)),
        ..Default::default()
    };

    // Control: on a fresh database, migrations run under these session
    // settings fail (sqlx runs each migration file as one statement).
    let control = fresh_database(&url).await;
    let opts = cfg.apply(control.parse().unwrap());
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .unwrap();
    let res = sqlx::migrate!("./migrations").run(&pool).await;
    assert!(
        res.is_err(),
        "control: a 1 ms statement_timeout must break the migrations"
    );
    pool.close().await;

    // The store migrates a fresh database on a connection without the
    // session timeouts, then serves with them.
    let fresh = fresh_database(&url).await;
    let store = PostgresStore::connect_with_config(&fresh, &cfg)
        .await
        .expect("connect must not run migrations under statement_timeout");
    drop(store);
    // Checked on a plain connection: the store's own sessions would cancel
    // this query after 1 ms.
    let mut check = sqlx::PgConnection::connect(&fresh).await.unwrap();
    let tables: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.tables WHERE table_name IN ('events', 'aggregates', 'idempotency')",
    )
    .fetch_one(&mut check)
    .await
    .unwrap();
    assert_eq!(tables, 3);
}

/// A new, empty database on the test server; returns its URL.
async fn fresh_database(url: &str) -> String {
    let name = unique("esp_mig").replace('-', "_");
    let mut admin = sqlx::PgConnection::connect(url).await.unwrap();
    sqlx::query(&format!("CREATE DATABASE {name}"))
        .execute(&mut admin)
        .await
        .unwrap();
    let (path, query) = url.split_once('?').unwrap_or((url, ""));
    let (base, _db) = path.rsplit_once('/').unwrap();
    let query = if query.is_empty() {
        String::new()
    } else {
        format!("?{query}")
    };
    format!("{base}/{name}{query}")
}

/// Completed operations hand their connection back: with one connection,
/// many sequential appends reuse it.
#[tokio::test]
async fn completed_operations_return_their_connection_to_the_pool() {
    let url = common::get_test_database_url().await;
    let cfg = PostgresConfig {
        max_connections: 1,
        ..Default::default()
    };
    let store = PostgresStore::connect_with_config(&url, &cfg)
        .await
        .unwrap();
    let tenant = unique("t-reuse");
    for i in 0..5 {
        store
            .append(append_req(&tenant, &format!("a{i}")))
            .await
            .unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while store.pool().num_idle() != 1 {
        assert!(Instant::now() < deadline, "connection was not returned");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(store.pool().size(), 1);
}

/// Holds the tenant's append-order lock from outside the store, so an append
/// for that tenant blocks exactly like it would behind a stalled holder.
async fn hold_append_lock(url: &str, tenant: &str) -> sqlx::PgConnection {
    let mut conn = sqlx::PgConnection::connect(url).await.unwrap();
    sqlx::query("SELECT pg_advisory_lock($1, hashtext($2))")
        .bind(APPEND_ORDER_LOCK_NAMESPACE)
        .bind(tenant)
        .execute(&mut conn)
        .await
        .unwrap();
    conn
}

async fn assert_blocked_append_surfaces(cfg: PostgresConfig, sqlstate: &str, bound: Duration) {
    let url = common::get_test_database_url().await;
    let store = PostgresStore::connect_with_config(&url, &cfg)
        .await
        .unwrap();
    let tenant = unique("t-lock");
    let mut holder = hold_append_lock(&url, &tenant).await;

    let started = Instant::now();
    let res = tokio::time::timeout(
        Duration::from_secs(30),
        store.append(append_req(&tenant, "a")),
    )
    .await
    .expect("a blocked append must not hang");
    let took = started.elapsed();
    match res {
        Err(StoreError::Unavailable(msg)) => assert!(msg.contains(sqlstate), "{msg}"),
        other => panic!("expected UNAVAILABLE ({sqlstate}), got {other:?}"),
    }
    assert!(took < bound, "surfaced after {took:?}, bound {bound:?}");

    // Nothing was written; once the holder goes away the same append works.
    sqlx::query("SELECT pg_advisory_unlock_all()")
        .execute(&mut holder)
        .await
        .unwrap();
    let ok = store.append(append_req(&tenant, "a")).await.unwrap();
    assert_eq!(ok.last_aggregate_nonce, 1);
}

#[tokio::test]
async fn append_blocked_on_the_order_lock_hits_lock_timeout() {
    let cfg = PostgresConfig {
        lock_timeout: Some(Duration::from_millis(300)),
        statement_timeout: Some(Duration::from_secs(20)),
        ..Default::default()
    };
    assert_blocked_append_surfaces(cfg, "55P03", Duration::from_secs(5)).await;
}

#[tokio::test]
async fn append_blocked_without_lock_timeout_hits_statement_timeout() {
    let cfg = PostgresConfig {
        lock_timeout: None,
        statement_timeout: Some(Duration::from_millis(400)),
        ..Default::default()
    };
    assert_blocked_append_surfaces(cfg, "57014", Duration::from_secs(5)).await;
}

#[tokio::test]
async fn idle_in_transaction_session_is_terminated_by_the_server() {
    let url = common::get_test_database_url().await;
    let cfg = PostgresConfig {
        idle_in_transaction_timeout: Some(Duration::from_millis(300)),
        ..Default::default()
    };
    let store = PostgresStore::connect_with_config(&url, &cfg)
        .await
        .unwrap();
    let mut conn = store.pool().acquire().await.unwrap();
    sqlx::query("BEGIN").execute(&mut *conn).await.unwrap();
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let err = sqlx::query("SELECT 1")
        .execute(&mut *conn)
        .await
        .expect_err("the server must have ended the idle transaction's session");
    // Postgres sends 25P03 then closes; sqlx may report either.
    let msg = format!("{err}");
    assert!(
        msg.contains("idle-in-transaction") || matches!(err, sqlx::Error::Io(_)),
        "{msg}"
    );
    conn.close_on_drop();
}

/// A caller that gives up first (gRPC deadline, client disconnect) drops the
/// append mid-query. Its connection must be closed, not handed back to the
/// pool: sqlx would ping it on return, and that ping waits for the abandoned
/// query (here: blocked on the order lock for up to statement_timeout, on a
/// stalled network forever), holding the pool slot all that time.
#[tokio::test]
async fn an_abandoned_append_frees_its_pool_slot() {
    let url = common::get_test_database_url().await;
    let cfg = PostgresConfig {
        max_connections: 1,
        lock_timeout: None,
        statement_timeout: Some(Duration::from_secs(30)),
        ..Default::default()
    };
    let store = PostgresStore::connect_with_config(&url, &cfg)
        .await
        .unwrap();
    let tenant = unique("t-abandon");
    let mut holder = hold_append_lock(&url, &tenant).await;

    let abandoned = tokio::time::timeout(
        Duration::from_millis(500),
        store.append(append_req(&tenant, "a")),
    )
    .await;
    assert!(abandoned.is_err(), "the append should still be blocked");

    let deadline = Instant::now() + Duration::from_secs(10);
    while store.pool().size() > 0 {
        assert!(
            Instant::now() < deadline,
            "the abandoned connection still holds the only pool slot"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    sqlx::query("SELECT pg_advisory_unlock_all()")
        .execute(&mut holder)
        .await
        .unwrap();
    let ok = tokio::time::timeout(
        Duration::from_secs(10),
        store.append(append_req(&tenant, "b")),
    )
    .await
    .expect("the pool must serve the next append")
    .unwrap();
    assert_eq!(ok.last_aggregate_nonce, 1);
}
