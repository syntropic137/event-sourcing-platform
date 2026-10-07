//! Black-holed database path (#368): the network between the event store and
//! Postgres stops delivering packets without closing anything (paused VM,
//! firewall drop). Appends and subscriptions must surface `UNAVAILABLE`
//! within the configured bound instead of hanging, the server must release
//! the per-tenant append-order lock on its own, and everything recovers once
//! the path returns. Run with `make -C event-store recovery-drill`.

mod drill;

use std::time::{Duration, Instant};

use drill::pg::DisposablePg;
use drill::projection::{Consumer, Stop};
use drill::proxy::FaultProxy;
use drill::server::EventStoreProc;
use drill::workload::{accounts_workload, append_all, assert_exactly_once, head, open, Cmd};
use drill::{eventually, unique, STEP};
use tonic::Code;

/// Server-side statement timeout for the proxied node. Its client deadline
/// is this plus the 5 s grace (`CLIENT_DEADLINE_GRACE`).
const STATEMENT_TIMEOUT_MS: u64 = 2_000;
const CLIENT_DEADLINE: Duration = Duration::from_millis(STATEMENT_TIMEOUT_MS + 5_000);
const ACQUIRE_TIMEOUT_MS: u64 = 3_000;
const IDLE_IN_TX_TIMEOUT_MS: u64 = 2_000;
/// Scheduling slack on a loaded CI host.
const SLACK: Duration = Duration::from_secs(5);
/// Fallback poll of a live subscription (`FALLBACK_POLL_SECS`).
const LIVE_POLL: Duration = Duration::from_secs(5);

const SLOW_PREFIX: &str = "blackhole-slow-";

struct Rig {
    pg: DisposablePg,
    proxy: FaultProxy,
    /// Event store whose database connections go through `proxy`.
    es: EventStoreProc,
    /// Event store connected directly to Postgres (another node).
    direct: EventStoreProc,
}

async fn rig(role: &str) -> Rig {
    let pg = DisposablePg::start(role).await;
    let proxy = FaultProxy::start(pg.addr()).await;
    let st = STATEMENT_TIMEOUT_MS.to_string();
    let acq = ACQUIRE_TIMEOUT_MS.to_string();
    let idle = IDLE_IN_TX_TIMEOUT_MS.to_string();
    let es = EventStoreProc::start_with_env(
        &proxy.rewrite_url(&pg.url(), pg.addr()),
        &[
            ("PG_STATEMENT_TIMEOUT_MS", &st),
            ("PG_ACQUIRE_TIMEOUT_MS", &acq),
            ("PG_IDLE_IN_TRANSACTION_TIMEOUT_MS", &idle),
        ],
    )
    .await;
    let direct = EventStoreProc::start(&pg.url()).await;
    Rig {
        pg,
        proxy,
        es,
        direct,
    }
}

/// Appends to `blackhole-slow-*` aggregates spend 1 s in their INSERT, so a
/// drill can stall the path while that append is inside its transaction
/// (after the order lock is taken).
async fn install_slow_insert(pool: &sqlx::PgPool) {
    sqlx::raw_sql(&format!(
        r#"
        CREATE OR REPLACE FUNCTION drill_blackhole_slow() RETURNS trigger AS $$
        BEGIN
            IF NEW.aggregate_id LIKE '{SLOW_PREFIX}%' THEN
                PERFORM pg_sleep(1);
            END IF;
            RETURN NEW;
        END
        $$ LANGUAGE plpgsql;
        DROP TRIGGER IF EXISTS trg_drill_blackhole_slow ON events;
        CREATE TRIGGER trg_drill_blackhole_slow AFTER INSERT ON events
            FOR EACH ROW EXECUTE FUNCTION drill_blackhole_slow();
        "#
    ))
    .execute(pool)
    .await
    .expect("install slow insert trigger");
}

async fn backends(pool: &sqlx::PgPool, predicate: &str) -> i64 {
    sqlx::query_scalar(&format!(
        "SELECT count(*) FROM pg_stat_activity \
         WHERE datname = current_database() AND pid <> pg_backend_pid() AND {predicate}"
    ))
    .fetch_one(pool)
    .await
    .expect("pg_stat_activity")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "recovery drill: needs Docker; run `make -C event-store recovery-drill`"]
async fn black_holed_path_during_append_surfaces_within_bound_and_releases_the_lock() {
    let rig = rig("blackhole-append").await;
    let pool = rig.pg.pool().await;
    install_slow_insert(&pool).await;
    let tenant = unique("t-bh-append");
    let slow: Cmd = open(&tenant, &format!("{SLOW_PREFIX}1"), "owner", "USD");

    // Start the slow append, wait until its INSERT runs on the server (the
    // order lock is held from just before it), then black-hole the path.
    let call = {
        let endpoint = rig.es.endpoint();
        let req = slow.req.clone();
        tokio::spawn(async move {
            let mut c = drill::server::connect(&endpoint).await;
            let started = Instant::now();
            let res = c.append(req).await;
            (res, started.elapsed())
        })
    };
    eventually("the slow INSERT to be running", STEP, || {
        let pool = pool.clone();
        async move {
            (backends(
                &pool,
                "state = 'active' AND query LIKE '%INSERT INTO events%'",
            )
            .await
                > 0)
            .then_some(())
        }
    })
    .await;
    rig.proxy.hold();
    let held_at = Instant::now();

    // The call surfaces UNAVAILABLE within the client deadline, not never.
    let (res, took) = tokio::time::timeout(CLIENT_DEADLINE + SLACK, call)
        .await
        .expect("append on a black-holed path must not hang")
        .unwrap();
    let status = res.expect_err("append on a black-holed path must fail");
    assert_eq!(status.code(), Code::Unavailable, "{status:?}");
    assert!(status.message().contains("client deadline"), "{status:?}");
    assert!(took < CLIENT_DEADLINE + SLACK, "took {took:?}");

    // The server notices the vanished client on its own: the session idles
    // in its transaction and is terminated, which rolls the append back and
    // releases the order lock, so another node can write the same tenant.
    let other = open(&tenant, "acct-other", "owner", "USD");
    let mut direct = rig.direct.client().await;
    let ok = tokio::time::timeout(STEP, direct.append(other.req.clone()))
        .await
        .expect("the order lock must be released without the stalled client")
        .expect("append through another node");
    assert!(ok.into_inner().last_global_nonce > 0);
    assert!(
        held_at.elapsed() < Duration::from_millis(IDLE_IN_TX_TIMEOUT_MS) + CLIENT_DEADLINE + SLACK,
        "lock released after {:?}",
        held_at.elapsed()
    );
    let slow_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM events WHERE tenant_id = $1 AND aggregate_id = $2",
    )
    .bind(&tenant)
    .bind(&slow.req.aggregate_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(slow_rows, 0, "the stalled append must have rolled back");

    // Recovery: the path returns, the same node serves appends again, and
    // the keyed retry of the stalled append commits exactly once.
    rig.proxy.restore();
    let mut client = rig.es.client().await;
    let retried = eventually("the proxied node to accept the retry", STEP, || {
        let mut c = client.clone();
        let req = slow.req.clone();
        async move { c.append(req).await.ok() }
    })
    .await
    .into_inner();
    assert_eq!(retried.last_aggregate_nonce, 1);
    let again = client.append(slow.req.clone()).await.unwrap().into_inner();
    assert_eq!(
        again.last_global_nonce, retried.last_global_nonce,
        "idempotent"
    );
    assert_exactly_once(&pool, &tenant, &[slow, other]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "recovery drill: needs Docker; run `make -C event-store recovery-drill`"]
async fn black_holed_path_during_live_subscription_surfaces_within_bound_and_resumes() {
    let rig = rig("blackhole-live").await;
    let pool = rig.pg.pool().await;
    let tenant = unique("t-bh-live");
    let (cmds, expected) = accounts_workload(&tenant, 3, 10);
    let (first, during) = cmds.split_at(20);
    append_all(&rig.es.endpoint(), first).await;

    let consumer = Consumer::new(pool.clone(), "balances", &tenant).await;
    let run = {
        let endpoint = rig.es.endpoint();
        let pool = pool.clone();
        let tenant = tenant.clone();
        tokio::spawn(async move {
            let c = Consumer::new(pool, "balances", &tenant).await;
            c.run(&endpoint, Stop::Never).await
        })
    };
    let caught_up = head(&pool, &tenant).await;
    eventually("replay to complete", STEP, || async {
        (consumer.checkpoint().await == caught_up).then_some(())
    })
    .await;

    // Black-hole while live. Another node keeps committing; its NOTIFYs to
    // the proxied node are held too, so the next live query (fallback poll)
    // runs into the stalled path and must surface.
    rig.proxy.hold();
    let held_at = Instant::now();
    append_all(&rig.direct.endpoint(), during).await;
    let bound = LIVE_POLL + CLIENT_DEADLINE + SLACK;
    let r = tokio::time::timeout(bound + SLACK, run)
        .await
        .expect("a live subscription on a black-holed path must not hang")
        .unwrap();
    let status = r.failed_status();
    assert_eq!(status.code(), Code::Unavailable, "{status:?}");
    assert!(
        status.message().contains("subscription live query failed"),
        "{status:?}"
    );
    assert!(
        status
            .message()
            .contains(&format!("resume from global_nonce {}", caught_up + 1)),
        "{status:?}"
    );
    assert!(
        held_at.elapsed() < bound,
        "surfaced after {:?}",
        held_at.elapsed()
    );
    assert_eq!(consumer.checkpoint().await, caught_up);

    // The path returns; resuming from the checkpoint loses nothing.
    rig.proxy.restore();
    let target = head(&pool, &tenant).await;
    let reports = consumer.run_to(&rig.es.endpoint(), target).await;
    let resumed: usize = reports.iter().map(|r| r.applied).sum();
    assert_eq!(resumed, during.len(), "{reports:?}");
    consumer.assert_complete().await;
    assert_eq!(consumer.state().await, expected);
    assert_exactly_once(&pool, &tenant, &cmds).await;
}
