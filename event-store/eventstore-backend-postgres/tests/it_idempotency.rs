//! Postgres-specific idempotency checks (ADR-028) that the generic
//! conformance suite cannot force:
//!
//! * Deterministic interleavings: every racing append is parked on the lock
//!   the re-checks exist for (the append-order advisory lock for a new
//!   stream, the stream row lock for an existing one) before the winner is
//!   let through, so the re-check paths are exercised, not just started.
//! * Rows written by earlier releases (legacy fingerprint) still match.
mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use eventstore_backend_postgres::PostgresStore;
use eventstore_core::fingerprint::canonical_metadata_bytes;
use eventstore_core::proto::{AppendRequest, AppendResponse, EventData, EventMetadata};
use eventstore_core::{EventStore, StoreError};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection};

/// Must equal `APPEND_ORDER_LOCK_NAMESPACE` in `store_postgres.rs`.
const APPEND_ORDER_LOCK_NAMESPACE: i32 = 0x0E5_1545;
const RACERS: usize = 6;

fn unique(prefix: &str) -> String {
    format!(
        "{prefix}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

fn event(tenant: &str, aggregate: &str, nonce: u64, tag: &str) -> EventData {
    EventData {
        meta: Some(EventMetadata {
            event_id: format!("{aggregate}-{nonce}-{tag}"),
            aggregate_id: aggregate.into(),
            aggregate_type: "IdemAccount".into(),
            aggregate_nonce: nonce,
            event_type: "Deposited".into(),
            event_version: 1,
            content_type: "application/json".into(),
            tenant_id: tenant.into(),
            headers: (0..4).map(|i| (format!("h{i}"), format!("v{i}"))).collect(),
            ..Default::default()
        }),
        payload: format!("{{\"tag\":\"{tag}\"}}").into_bytes(),
    }
}

fn request(tenant: &str, aggregate: &str, expected: u64, key: &str, tag: &str) -> AppendRequest {
    AppendRequest {
        tenant_id: tenant.into(),
        aggregate_id: aggregate.into(),
        aggregate_type: "IdemAccount".into(),
        expected_aggregate_nonce: expected,
        idempotency_key: key.into(),
        events: vec![event(tenant, aggregate, expected + 1, tag)],
    }
}

/// A store whose connections carry `app`, so the test can see them waiting.
async fn tagged_store(app: &str) -> (Arc<PostgresStore>, String) {
    let url = common::get_test_database_url().await;
    let sep = if url.contains('?') { '&' } else { '?' };
    let store = PostgresStore::connect_for_tests(&format!("{url}{sep}application_name={app}"))
        .await
        .expect("connect");
    (store, url)
}

async fn wait_for_lock_waiters(conn: &mut PgConnection, app: &str, n: i64) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let waiting: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity \
             WHERE application_name = $1 AND wait_event_type = 'Lock'",
        )
        .bind(app)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        if waiting == n {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "only {waiting}/{n} appends reached the lock"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn spawn_racers(
    store: &Arc<PostgresStore>,
    reqs: Vec<AppendRequest>,
) -> tokio::task::JoinSet<Result<AppendResponse, StoreError>> {
    let mut set = tokio::task::JoinSet::new();
    for req in reqs {
        let store = store.clone();
        set.spawn(async move { store.append(req).await });
    }
    set
}

async fn join_all(
    mut set: tokio::task::JoinSet<Result<AppendResponse, StoreError>>,
) -> Vec<Result<AppendResponse, StoreError>> {
    let mut out = Vec::new();
    while let Some(r) = set.join_next().await {
        out.push(r.expect("task"));
    }
    out
}

/// New stream: all racers pass the first checks (no rows to lock) and park
/// on the append-order lock held here. After release, one commits; the rest
/// must hit the post-lock re-check.
async fn park_on_advisory_lock(
    reqs: Vec<AppendRequest>,
    tenant: &str,
) -> Vec<Result<AppendResponse, StoreError>> {
    let app = unique("idem-adv");
    let (store, url) = tagged_store(&app).await;
    let mut holder = PgConnection::connect(&url).await.unwrap();
    sqlx::query("SELECT pg_advisory_lock($1, hashtext($2))")
        .bind(APPEND_ORDER_LOCK_NAMESPACE)
        .bind(tenant)
        .execute(&mut holder)
        .await
        .unwrap();
    let n = reqs.len() as i64;
    let set = spawn_racers(&store, reqs);
    wait_for_lock_waiters(&mut holder, &app, n).await;
    sqlx::query("SELECT pg_advisory_unlock($1, hashtext($2))")
        .bind(APPEND_ORDER_LOCK_NAMESPACE)
        .bind(tenant)
        .execute(&mut holder)
        .await
        .unwrap();
    join_all(set).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parked_identical_retries_on_new_stream_all_get_the_ack() {
    let (tenant, agg) = (unique("idem-t"), unique("idem-a"));
    let req = request(&tenant, &agg, 0, "k", "a");
    let results = park_on_advisory_lock(vec![req; RACERS], &tenant).await;
    let acks: Vec<AppendResponse> = results
        .into_iter()
        .map(|r| r.expect("every identical request gets the ack"))
        .collect();
    assert!(acks.windows(2).all(|w| w[0] == w[1]), "{acks:?}");
    assert_eq!(acks[0].last_aggregate_nonce, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parked_unkeyed_writers_on_new_stream_get_conflict_not_internal() {
    let (tenant, agg) = (unique("idem-t"), unique("idem-a"));
    let reqs = (0..RACERS)
        .map(|i| request(&tenant, &agg, 0, "", &format!("w{i}")))
        .collect();
    let results = park_on_advisory_lock(reqs, &tenant).await;
    let ok = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(ok, 1);
    for r in results.into_iter().filter(|r| r.is_err()) {
        match r {
            Err(StoreError::Concurrency {
                detail: Some(d), ..
            }) => assert_eq!(d.actual_last_aggregate_nonce, 1),
            other => panic!("expected Concurrency with detail, got {other:?}"),
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parked_identical_retries_on_existing_stream_all_get_the_ack() {
    let (tenant, agg) = (unique("idem-t"), unique("idem-a"));
    let app = unique("idem-row");
    let (store, url) = tagged_store(&app).await;
    store
        .append(request(&tenant, &agg, 0, "", "first"))
        .await
        .unwrap();

    // Hold the stream row lock: racers pass the key check, then park on it.
    let mut holder = PgConnection::connect(&url).await.unwrap();
    let mut tx = holder.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM aggregates WHERE tenant_id = $1 AND aggregate_id = $2 FOR UPDATE")
        .bind(&tenant)
        .bind(&agg)
        .execute(&mut *tx)
        .await
        .unwrap();
    let req = request(&tenant, &agg, 1, "k", "b");
    let set = spawn_racers(&store, vec![req; RACERS]);
    let mut observer = PgConnection::connect(&url).await.unwrap();
    wait_for_lock_waiters(&mut observer, &app, RACERS as i64).await;
    tx.commit().await.unwrap();

    let acks: Vec<AppendResponse> = join_all(set)
        .await
        .into_iter()
        .map(|r| r.expect("every identical request gets the ack"))
        .collect();
    assert!(acks.windows(2).all(|w| w[0] == w[1]), "{acks:?}");
    assert_eq!(acks[0].last_aggregate_nonce, 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rows_with_legacy_fingerprints_still_match_retries() {
    let (tenant, agg) = (unique("idem-t"), unique("idem-a"));
    let (store, _) = tagged_store(&unique("idem-legacy")).await;
    let req = request(&tenant, &agg, 0, "k-legacy", "a");
    let ack = store.append(req.clone()).await.unwrap();

    // Rewrite the row as an earlier release stored it: unframed SHA-256 of
    // canonical metadata + payload, 32 bytes.
    let mut h = Sha256::new();
    for ev in &req.events {
        h.update(canonical_metadata_bytes(ev.meta.as_ref().unwrap()));
        h.update(&ev.payload);
    }
    let legacy = h.finalize().to_vec();
    let updated = sqlx::query(
        "UPDATE idempotency SET request_fingerprint = $4 \
         WHERE tenant_id = $1 AND aggregate_id = $2 AND idempotency_key = $3",
    )
    .bind(&tenant)
    .bind(&agg)
    .bind("k-legacy")
    .bind(&legacy)
    .execute(store.pool())
    .await
    .unwrap();
    assert_eq!(updated.rows_affected(), 1);

    assert_eq!(store.append(req).await.unwrap(), ack, "legacy row matches");
    match store
        .append(request(&tenant, &agg, 0, "k-legacy", "different"))
        .await
    {
        Err(StoreError::AlreadyExists(_)) => {}
        other => panic!("expected AlreadyExists, got {other:?}"),
    }
}
