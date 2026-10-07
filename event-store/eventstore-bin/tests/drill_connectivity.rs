//! Postgres connectivity loss during replay and live consumption, and
//! consumer resume from persisted checkpoints (#355).
//!
//! The event store reaches Postgres through an in-process fault proxy, so a
//! drill can hold a query in flight and then cut every connection. A second
//! event-store process connected directly (another node) keeps writing
//! during the outage. Run with `make -C event-store recovery-drill`.

mod drill;

use std::time::Duration;

use drill::pg::DisposablePg;
use drill::projection::{Consumer, Outcome, Stop};
use drill::proxy::FaultProxy;
use drill::server::EventStoreProc;
use drill::workload::{accounts_workload, append_all, assert_exactly_once, head, Cmd};
use drill::{eventually, unique, STEP};
use tonic::Code;

struct Rig {
    pg: DisposablePg,
    proxy: FaultProxy,
    /// Event store whose database connections go through `proxy`.
    es: EventStoreProc,
    /// Event store connected directly to Postgres (unaffected by `proxy`).
    direct: EventStoreProc,
}

async fn rig(role: &str) -> Rig {
    let pg = DisposablePg::start(role).await;
    let proxy = FaultProxy::start(pg.addr()).await;
    let es = EventStoreProc::start(&proxy.rewrite_url(&pg.url(), pg.addr())).await;
    let direct = EventStoreProc::start(&pg.url()).await;
    Rig {
        pg,
        proxy,
        es,
        direct,
    }
}

fn assert_unavailable(status: &tonic::Status, phase: &str, resume_from: u64) {
    assert_eq!(status.code(), Code::Unavailable, "{status:?}");
    let msg = status.message();
    assert!(
        msg.contains(&format!("subscription {phase} query failed")),
        "{msg}"
    );
    assert!(
        msg.contains(&format!("resume from global_nonce {resume_from}")),
        "{msg}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "recovery drill: needs Docker; run `make -C event-store recovery-drill`"]
async fn db_outage_during_replay_surfaces_and_resume_from_checkpoint_completes() {
    let rig = rig("replay-outage").await;
    let pool = rig.pg.pool().await;
    let tenant = unique("t-replay");
    let (cmds, expected) = accounts_workload(&tenant, 6, 24);
    let (before, during): (&[Cmd], &[Cmd]) = cmds.split_at(6 * 20);
    append_all(&rig.es.endpoint(), before).await;

    // A consumer processes part of the log, then crashes.
    let consumer = Consumer::new(pool.clone(), "balances", &tenant).await;
    let r1 = consumer
        .run(&rig.es.endpoint(), Stop::AfterApplied(50))
        .await;
    assert!(matches!(r1.outcome, Outcome::Crashed), "{r1:?}");
    let cp = consumer.checkpoint().await;
    assert!(cp > 0);

    // It restarts while the database path stalls: the replay traffic is
    // held by the proxy (it never reaches Postgres), then every connection
    // is cut, so the replay query fails on the wire.
    rig.proxy.hold();
    let sent_before = rig.proxy.client_bytes();
    let run2 = {
        let endpoint = rig.es.endpoint();
        let pool = pool.clone();
        let tenant = tenant.clone();
        tokio::spawn(async move {
            let c = Consumer::new(pool, "balances", &tenant).await;
            c.run(&endpoint, Stop::AtGlobalNonce(u64::MAX)).await
        })
    };
    eventually("the replay query to reach the proxy", STEP, || {
        let sent = rig.proxy.client_bytes();
        async move { (sent > sent_before).then_some(()) }
    })
    .await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    rig.proxy.cut();

    let r2 = tokio::time::timeout(STEP, run2).await.unwrap().unwrap();
    assert_unavailable(r2.failed_status(), "replay", cp + 1);
    assert_eq!(r2.delivered, 0, "{r2:?}");
    assert_eq!(r2.caught_up_markers, 0, "no false catch-up: {r2:?}");
    assert_eq!(consumer.checkpoint().await, cp, "checkpoint untouched");

    // Writes through the cut node fail visibly; another node keeps writing.
    let mut cut_client = rig.es.client().await;
    let failed = tokio::time::timeout(STEP, cut_client.append(during[0].req.clone()))
        .await
        .expect("append through a cut database must fail, not hang");
    assert!(failed.is_err(), "{failed:?}");
    append_all(&rig.direct.endpoint(), during).await;

    // Connectivity returns; resume from the saved checkpoint.
    rig.proxy.restore();
    let target = head(&pool, &tenant).await;
    let reports = consumer.run_to(&rig.es.endpoint(), target).await;
    let resumed: usize = reports.iter().map(|r| r.applied).sum();
    let dups: usize = reports.iter().map(|r| r.duplicates_skipped).sum();
    assert_eq!(resumed, cmds.len() - 50, "{reports:?}");
    assert_eq!(dups, 0, "checkpoint was exact: {reports:?}");
    assert_eq!(reports[0].from, cp + 1);

    consumer.assert_complete().await;
    assert_eq!(consumer.state().await, expected);
    assert_exactly_once(&pool, &tenant, &cmds).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "recovery drill: needs Docker; run `make -C event-store recovery-drill`"]
async fn db_outage_during_live_consumption_surfaces_and_resume_completes() {
    let rig = rig("live-outage").await;
    let pool = rig.pg.pool().await;
    let tenant = unique("t-live");
    let (cmds, expected) = accounts_workload(&tenant, 4, 15);
    let (first, rest) = cmds.split_at(30);
    let (live, during) = rest.split_at(10);
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

    // Live delivery works.
    append_all(&rig.es.endpoint(), live).await;
    let live_head = head(&pool, &tenant).await;
    eventually("live events to be applied", STEP, || async {
        (consumer.checkpoint().await == live_head).then_some(())
    })
    .await;

    // Outage while live; another node commits events the consumer must not lose.
    rig.proxy.cut();
    append_all(&rig.direct.endpoint(), during).await;
    let r = tokio::time::timeout(STEP, run).await.unwrap().unwrap();
    assert_unavailable(r.failed_status(), "live", live_head + 1);
    assert_eq!(consumer.checkpoint().await, live_head);

    rig.proxy.restore();
    let target = head(&pool, &tenant).await;
    let reports = consumer.run_to(&rig.es.endpoint(), target).await;
    let resumed: usize = reports.iter().map(|r| r.applied).sum();
    assert_eq!(resumed, during.len(), "{reports:?}");
    consumer.assert_complete().await;
    assert_eq!(consumer.state().await, expected);
}

/// The consumer saves its checkpoint only every 7 events, so a restart
/// re-delivers events it already applied; the projection must skip them.
/// The event store is also SIGKILLed under a live subscription.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "recovery drill: needs Docker; run `make -C event-store recovery-drill`"]
async fn lagging_checkpoint_redelivery_and_eventstore_kill_yield_idempotent_projection() {
    let pg = DisposablePg::start("cp-lag").await;
    let pool = pg.pool().await;
    let mut es = EventStoreProc::start(&pg.url()).await;
    let tenant = unique("t-cp-lag");
    let (cmds, expected) = accounts_workload(&tenant, 6, 15);
    append_all(&es.endpoint(), &cmds[..40]).await;

    let mut consumer = Consumer::new(pool.clone(), "balances", &tenant).await;
    consumer.checkpoint_every = 7;
    let r1 = consumer.run(&es.endpoint(), Stop::AfterApplied(40)).await;
    assert!(matches!(r1.outcome, Outcome::Crashed), "{r1:?}");
    // Checkpoints were saved after events 7, 14, ..., 35.
    let applied_g: Vec<i64> = sqlx::query_scalar(
        "SELECT global_nonce FROM drill_applied WHERE tenant_id = $1 ORDER BY global_nonce",
    )
    .bind(&tenant)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(applied_g.len(), 40);
    assert_eq!(consumer.checkpoint().await, applied_g[34] as u64);

    // Restart with no new writes: the 5 redelivered events are skipped and
    // still move the checkpoint to the head, so it does not stay behind.
    let head40 = head(&pool, &tenant).await;
    assert_eq!(head40, applied_g[39] as u64);
    let reports = consumer.run_to(&es.endpoint(), head40).await;
    let dups: usize = reports.iter().map(|r| r.duplicates_skipped).sum();
    let applied: usize = reports.iter().map(|r| r.applied).sum();
    assert_eq!((dups, applied), (5, 0), "{reports:?}");
    assert_eq!(consumer.checkpoint().await, head40);

    // More writes; resume, then the store is killed under the live
    // subscription. Checkpoints again every 7 events, so they lag.
    append_all(&es.endpoint(), &cmds[40..80]).await;
    let run2 = {
        let endpoint = es.endpoint();
        let pool = pool.clone();
        let tenant = tenant.clone();
        tokio::spawn(async move {
            let mut c = Consumer::new(pool, "balances", &tenant).await;
            c.checkpoint_every = 7;
            c.run(&endpoint, Stop::Never).await
        })
    };
    eventually("consumer to apply the backlog", STEP, || async {
        (consumer.applied_count().await == 80).then_some(())
    })
    .await;
    es.kill();
    let r2 = tokio::time::timeout(STEP, run2).await.unwrap().unwrap();
    assert_eq!(r2.duplicates_skipped, 0, "{r2:?}");
    assert_eq!(r2.applied, 40, "{r2:?}");
    assert!(
        matches!(r2.outcome, Outcome::Failed(_)),
        "a killed store must end the stream with an error: {r2:?}"
    );

    es.restart().await;
    append_all(&es.endpoint(), &cmds[80..]).await;
    let target = head(&pool, &tenant).await;
    // checkpoint_every = 1 for the final run so the checkpoint lands on head.
    consumer.checkpoint_every = 1;
    let reports = consumer.run_to(&es.endpoint(), target).await;
    let dups: usize = reports.iter().map(|r| r.duplicates_skipped).sum();
    assert!(
        dups > 0,
        "lagging checkpoint must cause redelivery: {reports:?}"
    );
    assert_eq!(consumer.checkpoint().await, target);
    consumer.assert_complete().await;
    assert_eq!(consumer.state().await, expected);
}
