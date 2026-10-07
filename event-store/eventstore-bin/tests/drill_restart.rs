//! Restart recovery and retry safety drills (#355).
//!
//! The real `eventstore-bin` process is SIGKILLed around the append /
//! acknowledgment boundaries and restarted against the same disposable
//! Postgres; persisted records are then reconciled against what clients were
//! told. Run with `make -C event-store recovery-drill`.

mod drill;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use drill::pg::{assert_durability_settings, DisposablePg};
use drill::projection::Consumer;
use drill::proxy::FaultProxy;
use drill::server::{connect, EventStoreProc};
use drill::workload::{
    accounts_workload, append_all, append_until_acked, assert_exactly_once, committed_count,
    global_nonce_of, head, idempotency_rows, open, read_stream, Cmd, RetryStats,
};
use drill::{eventually, unique, STEP};
use eventstore_proto::gen::{AppendResponse, EventData, EventMetadata, ReadStreamRequest};
use sqlx::PgPool;
use tonic::Code;

/// Mirrors `APPEND_ORDER_LOCK_NAMESPACE` in store_postgres.rs. If it drifts,
/// `waiting_backend` times out and the drill fails; it cannot pass falsely.
const APPEND_ORDER_LOCK_NAMESPACE: i32 = 0x0E5_1545;

/// The Postgres backend (pid) of an event-store append that is blocked on a
/// lock of kind `wait_event` while running a statement containing `stmt`.
async fn waiting_backend(pool: &PgPool, wait_event: &str, stmt: &str) -> i32 {
    eventually(
        &format!("an append blocked on {wait_event} in `{stmt}`"),
        STEP,
        || async {
            sqlx::query_scalar::<_, i32>(
                "SELECT pid FROM pg_stat_activity
                  WHERE wait_event_type = 'Lock' AND wait_event = $1
                    AND query LIKE '%' || $2 || '%' AND pid <> pg_backend_pid()",
            )
            .bind(wait_event)
            .bind(stmt)
            .fetch_optional(pool)
            .await
            .unwrap()
        },
    )
    .await
}

async fn wait_backend_gone(pool: &PgPool, pid: i32) {
    eventually(&format!("backend {pid} to exit"), STEP, || async {
        let alive: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE pid = $1)")
                .bind(pid)
                .fetch_one(pool)
                .await
                .unwrap();
        (!alive).then_some(())
    })
    .await
}

/// Stored event equals what the client sent, at the acknowledged position.
fn assert_stored_as_sent(stored: &EventData, sent: &EventData, acked_global: u64) {
    let s = stored.meta.as_ref().unwrap();
    let m = sent.meta.as_ref().unwrap();
    assert_eq!(s.global_nonce, acked_global, "{}", m.event_id);
    assert!(s.recorded_time_unix_ms > 0, "{}", m.event_id);
    // Every client-supplied field round-trips; only the two server-assigned
    // fields differ from the request.
    let expected = EventMetadata {
        global_nonce: acked_global,
        recorded_time_unix_ms: s.recorded_time_unix_ms,
        ..m.clone()
    };
    assert_eq!(s, &expected, "metadata of {}", m.event_id);
    assert_eq!(stored.payload, sent.payload, "payload of {}", m.event_id);
}

/// Every acknowledged command is stored exactly as sent at its acknowledged
/// position (single-event commands).
async fn reconcile_acked(endpoint: &str, tenant: &str, cmds: &[Cmd], acks: &[AppendResponse]) {
    let mut by_stream = std::collections::HashMap::<String, Vec<EventData>>::new();
    for (cmd, ack) in cmds.iter().zip(acks) {
        let agg = &cmd.req.aggregate_id;
        if !by_stream.contains_key(agg) {
            by_stream.insert(agg.clone(), read_stream(endpoint, tenant, agg).await);
        }
        let stream = &by_stream[agg];
        let sent = &cmd.req.events[0];
        let nonce = sent.meta.as_ref().unwrap().aggregate_nonce as usize;
        let stored = stream
            .get(nonce - 1)
            .unwrap_or_else(|| panic!("acknowledged event missing: {}", cmd.key()));
        assert_eq!(ack.last_aggregate_nonce, nonce as u64);
        assert_stored_as_sent(stored, sent, ack.last_global_nonce);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "recovery drill: needs Docker; run `make -C event-store recovery-drill`"]
async fn acked_appends_survive_eventstore_and_postgres_crashes() {
    let pg = DisposablePg::start("acked").await;
    assert_durability_settings(&pg.pool().await).await;
    let mut es = EventStoreProc::start(&pg.url()).await;
    let tenant = unique("t-acked");
    let (cmds, expected) = accounts_workload(&tenant, 5, 20);
    let (phase1, rest) = cmds.split_at(60);
    let (phase2, phase3) = rest.split_at(20);

    // 1. Acknowledged appends, then SIGKILL both the event store and Postgres.
    let acks1 = append_all(&es.endpoint(), phase1).await;
    es.kill();
    pg.crash();
    pg.start_again().await;
    let pool = pg.pool().await;
    assert_durability_settings(&pool).await;
    es.restart().await;
    reconcile_acked(&es.endpoint(), &tenant, phase1, &acks1).await;

    // 2. Postgres crashes under a running event store: the same process must
    //    recover its pool without a restart; retries are safe.
    let acks2 = append_all(&es.endpoint(), phase2).await;
    let pid = es.pid();
    pg.crash();
    pg.start_again().await;
    let mut client = None;
    let mut stats = RetryStats::default();
    let mut acks3 = Vec::new();
    for c in phase3 {
        acks3.push(append_until_acked(&es.endpoint(), &mut client, c, &mut stats, None).await);
    }
    assert_eq!(es.pid(), pid, "event store was not restarted");
    eprintln!("retries after postgres crash: {stats:?}");

    let all_acks: Vec<AppendResponse> = acks1.into_iter().chain(acks2).chain(acks3).collect();
    reconcile_acked(&es.endpoint(), &tenant, &cmds, &all_acks).await;
    assert_exactly_once(&pool, &tenant, &cmds).await;

    let consumer = Consumer::new(pool.clone(), "balances", &tenant).await;
    consumer
        .run_to(&es.endpoint(), head(&pool, &tenant).await)
        .await;
    consumer.assert_complete().await;
    assert_eq!(consumer.state().await, expected);
}

/// Kill while the append waits for the per-tenant ordering lock, i.e. after
/// the request reached the store but before any row was written.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "recovery drill: needs Docker; run `make -C event-store recovery-drill`"]
async fn kill_before_insert_leaves_nothing_and_retry_commits_once() {
    let pg = DisposablePg::start("before-insert").await;
    let pool = pg.pool().await;
    let mut es = EventStoreProc::start(&pg.url()).await;
    let tenant = unique("t-before-insert");
    let cmd = open(&tenant, "acct-1", "One", "USD");

    let mut side = pool.acquire().await.unwrap();
    sqlx::query("SELECT pg_advisory_lock($1, hashtext($2))")
        .bind(APPEND_ORDER_LOCK_NAMESPACE)
        .bind(&tenant)
        .execute(&mut *side)
        .await
        .unwrap();

    let endpoint = es.endpoint();
    let req = cmd.req.clone();
    let inflight = tokio::spawn(async move { connect(&endpoint).await.append(req).await });
    let backend = waiting_backend(&pool, "advisory", "pg_advisory_xact_lock").await;

    es.kill();
    let res = tokio::time::timeout(STEP, inflight).await.unwrap().unwrap();
    assert!(res.is_err(), "client must not get an ack: {res:?}");

    // Let the orphaned transaction run on: it finds its client gone and rolls back.
    sqlx::query("SELECT pg_advisory_unlock($1, hashtext($2))")
        .bind(APPEND_ORDER_LOCK_NAMESPACE)
        .bind(&tenant)
        .execute(&mut *side)
        .await
        .unwrap();
    wait_backend_gone(&pool, backend).await;
    assert_eq!(committed_count(&pool, &tenant, &cmd.event_ids()).await, 0);
    assert_eq!(idempotency_rows(&pool, &tenant, cmd.key()).await, 0);

    es.restart().await;
    let mut client = es.client().await;
    let ack = client.append(cmd.req.clone()).await.unwrap().into_inner();
    let again = client.append(cmd.req.clone()).await.unwrap().into_inner();
    assert_eq!(ack, again, "identical retry returns the recorded result");
    assert_exactly_once(&pool, &tenant, std::slice::from_ref(&cmd)).await;
}

/// Kill after the events were inserted in the append transaction, before
/// COMMIT: the append is blocked on its idempotency-record insert by a side
/// transaction holding a conflicting uncommitted row.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "recovery drill: needs Docker; run `make -C event-store recovery-drill`"]
async fn kill_after_insert_before_commit_rolls_back_and_retry_commits_once() {
    let pg = DisposablePg::start("before-commit").await;
    let pool = pg.pool().await;
    let mut es = EventStoreProc::start(&pg.url()).await;
    let tenant = unique("t-before-commit");
    let setup = open(&tenant, "acct-1", "One", "USD");
    append_all(&es.endpoint(), std::slice::from_ref(&setup)).await;
    let cmd = drill::workload::deposit(&tenant, "acct-1", 2, 4_200);

    let mut side = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO idempotency (tenant_id, aggregate_id, idempotency_key, request_fingerprint,
                                  first_committed_nonce, last_committed_nonce, last_global_nonce)
         VALUES ($1, $2, $3, '\\x00'::bytea, 1, 1, 1)",
    )
    .bind(&tenant)
    .bind("acct-1")
    .bind(cmd.key())
    .execute(&mut *side)
    .await
    .unwrap();

    let endpoint = es.endpoint();
    let req = cmd.req.clone();
    let inflight = tokio::spawn(async move { connect(&endpoint).await.append(req).await });
    // The events and the aggregate head are already written in the append's
    // transaction when it reaches the idempotency insert.
    let backend = waiting_backend(&pool, "transactionid", "INSERT INTO idempotency").await;

    es.kill();
    let res = tokio::time::timeout(STEP, inflight).await.unwrap().unwrap();
    assert!(res.is_err(), "client must not get an ack: {res:?}");
    side.rollback().await.unwrap();
    wait_backend_gone(&pool, backend).await;

    assert_eq!(committed_count(&pool, &tenant, &cmd.event_ids()).await, 0);
    assert_eq!(idempotency_rows(&pool, &tenant, cmd.key()).await, 0);
    let last_nonce: i64 = sqlx::query_scalar(
        "SELECT last_nonce FROM aggregates WHERE tenant_id = $1 AND aggregate_id = 'acct-1'",
    )
    .bind(&tenant)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(last_nonce, 1, "aggregate head rolled back with the events");

    es.restart().await;
    let mut client = es.client().await;
    let ack = client.append(cmd.req.clone()).await.unwrap().into_inner();
    assert_eq!(ack.last_aggregate_nonce, 2);
    let again = client.append(cmd.req.clone()).await.unwrap().into_inner();
    assert_eq!(ack, again);
    assert_exactly_once(&pool, &tenant, &[setup, cmd]).await;
}

/// The append commits but the process dies before the client sees the
/// acknowledgment (responses are held by a proxy until the kill).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "recovery drill: needs Docker; run `make -C event-store recovery-drill`"]
async fn lost_ack_after_commit_identical_retry_is_idempotent() {
    let pg = DisposablePg::start("lost-ack").await;
    let pool = pg.pool().await;
    let mut es = EventStoreProc::start(&pg.url()).await;
    let tenant = unique("t-lost-ack");
    let cmd = open(&tenant, "acct-1", "One", "USD");

    let proxy = FaultProxy::start(es.addr).await;
    let mut client = connect(&proxy.endpoint()).await;
    // Warm the HTTP/2 connection so only the append's response is held.
    client
        .read_stream(ReadStreamRequest {
            tenant_id: tenant.clone(),
            aggregate_id: "warm".into(),
            from_aggregate_nonce: 1,
            max_count: 1,
            forward: true,
        })
        .await
        .unwrap();
    proxy.hold_responses();
    let req = cmd.req.clone();
    let mut inflight_client = client.clone();
    let inflight = tokio::spawn(async move { inflight_client.append(req).await });

    let ids = cmd.event_ids();
    eventually("the append to commit", STEP, || async {
        (committed_count(&pool, &tenant, &ids).await == 1).then_some(())
    })
    .await;
    es.kill();
    proxy.cut();
    let res = tokio::time::timeout(STEP, inflight).await.unwrap().unwrap();
    assert!(
        res.is_err(),
        "the acknowledgment must have been lost: {res:?}"
    );
    let committed_at = global_nonce_of(&pool, &tenant, &ids[0]).await.unwrap();

    es.restart().await;
    let mut client = es.client().await;

    // Without a key, optimistic concurrency rejects the duplicate.
    let unkeyed = client.append(cmd.without_key()).await.unwrap_err();
    assert_eq!(unkeyed.code(), Code::Aborted, "{unkeyed:?}");

    // Same key, different content: refused, not applied.
    let mut altered = cmd.req.clone();
    altered.events[0].payload = br#"{"owner":"Mallory","currency":"USD"}"#.to_vec();
    let refused = client.append(altered).await.unwrap_err();
    assert_eq!(refused.code(), Code::AlreadyExists, "{refused:?}");

    // Identical retry: the original commit's result, no new effect.
    let ack = client.append(cmd.req.clone()).await.unwrap().into_inner();
    assert_eq!(ack.last_global_nonce, committed_at);
    assert_eq!(ack.last_aggregate_nonce, 1);
    assert_exactly_once(&pool, &tenant, std::slice::from_ref(&cmd)).await;
}

/// Repeatedly SIGKILL and restart the event store while a client writes,
/// retrying each command with its stable idempotency key until acknowledged.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "recovery drill: needs Docker; run `make -C event-store recovery-drill`"]
async fn kill_storm_during_writes_reconciles_exactly_once() {
    const KILLS: usize = 6;
    let pg = DisposablePg::start("storm").await;
    let pool = pg.pool().await;
    let mut es = EventStoreProc::start(&pg.url()).await;
    let tenant = unique("t-storm");
    let (cmds, expected) = accounts_workload(&tenant, 8, 40);
    let total = cmds.len();

    let progress = Arc::new(AtomicUsize::new(0));
    let in_flight = Arc::new(AtomicBool::new(false));
    let writer = {
        let endpoint = es.endpoint();
        let cmds = cmds.clone();
        let progress = progress.clone();
        let in_flight = in_flight.clone();
        tokio::spawn(async move {
            let mut client = None;
            let mut stats = RetryStats::default();
            let mut acks = Vec::new();
            for c in &cmds {
                let ack =
                    append_until_acked(&endpoint, &mut client, c, &mut stats, Some(&in_flight))
                        .await;
                acks.push(ack);
                progress.fetch_add(1, Ordering::SeqCst);
            }
            (acks, stats)
        })
    };

    // Kill at fixed progress points, only while a client append call is in
    // flight (it may not have reached the server yet; the server-side
    // boundaries are pinned deterministically by the drills above),
    // after a varying delay so the kill lands at different points of it.
    let mut kills_in_flight = 0;
    for k in 0..KILLS {
        let at = (k + 1) * total / (KILLS + 2);
        eventually("writer progress", Duration::from_secs(120), || {
            let p = progress.load(Ordering::SeqCst);
            async move { (p >= at).then_some(()) }
        })
        .await;
        let deadline = std::time::Instant::now() + STEP;
        while !in_flight.load(Ordering::SeqCst) {
            assert!(std::time::Instant::now() < deadline, "no append in flight");
            std::hint::spin_loop();
        }
        std::thread::sleep(Duration::from_micros(250 * k as u64));
        assert!(!writer.is_finished(), "writer finished before kill {k}");
        if in_flight.load(Ordering::SeqCst) {
            kills_in_flight += 1;
        }
        es.kill();
        es.restart().await;
    }
    let (acks, stats) = tokio::time::timeout(Duration::from_secs(300), writer)
        .await
        .expect("writer finished")
        .unwrap();
    eprintln!("kills={KILLS} in_flight_at_kill={kills_in_flight} retry stats: {stats:?}");
    // Each in-flight kill fails the writer's pending call. Allow for a call
    // that completed between the flag check and the kill.
    assert!(
        kills_in_flight >= KILLS / 2,
        "kills_in_flight={kills_in_flight}"
    );
    assert!(
        stats.transport_or_unavailable >= KILLS / 2,
        "in-flight kills were observed by the writer: {stats:?}"
    );

    assert_exactly_once(&pool, &tenant, &cmds).await;
    for (c, ack) in cmds.iter().zip(&acks) {
        assert_eq!(ack.last_aggregate_nonce, c.last_nonce(), "{}", c.key());
        let g = global_nonce_of(&pool, &tenant, &c.event_ids()[0]).await;
        assert_eq!(
            g,
            Some(ack.last_global_nonce),
            "ack of {} names its commit",
            c.key()
        );
    }
    reconcile_acked(&es.endpoint(), &tenant, &cmds, &acks).await;

    let consumer = Consumer::new(pool.clone(), "balances", &tenant).await;
    consumer
        .run_to(&es.endpoint(), head(&pool, &tenant).await)
        .await;
    consumer.assert_complete().await;
    assert_eq!(consumer.state().await, expected);
}
