//! A live subscriber must see every committed event, whatever order the
//! appending transactions commit in (syntropic137#1545).
//!
//! `global_nonce` comes from a sequence, so it is handed out when an append
//! INSERTs, not when it commits. If a slow append holds nonce N while a fast
//! one commits N+1, a subscriber polling `global_nonce > cursor` sees N+1,
//! advances past N, and never sees N even after it commits. No error is
//! raised anywhere: the event is in the store, just behind the cursor.
mod common;

use std::time::Duration;

use eventstore_backend_postgres::PostgresStore;
use eventstore_core::proto;
use eventstore_core::EventStore;
use futures::StreamExt;

const TENANT: &str = "tenant-commit-order";
const SLOW_PREFIX: &str = "CommitOrderSlow-";

fn event(aggregate_id: &str, event_type: &str) -> proto::EventData {
    proto::EventData {
        meta: Some(proto::EventMetadata {
            event_id: format!("{aggregate_id}-{event_type}"),
            aggregate_id: aggregate_id.into(),
            aggregate_type: "CommitOrder".into(),
            aggregate_nonce: 1,
            event_type: event_type.into(),
            event_version: 1,
            content_type: "application/json".into(),
            tenant_id: TENANT.into(),
            ..Default::default()
        }),
        payload: b"{}".to_vec(),
    }
}

fn append_request(aggregate_id: &str, event_type: &str) -> proto::AppendRequest {
    proto::AppendRequest {
        tenant_id: TENANT.into(),
        aggregate_id: aggregate_id.into(),
        aggregate_type: "CommitOrder".into(),
        expected_aggregate_nonce: 0,
        idempotency_key: String::new(),
        events: vec![event(aggregate_id, event_type)],
    }
}

/// Single-bigint advisory key the test holds to keep the slow append open.
/// The store's ordering lock uses the two-int key form, a separate keyspace.
const GATE_KEY: i64 = 0x0E51_5450_0001;

/// How many other backends in this database match `predicate`.
async fn backends_where(store: &PostgresStore, predicate: &str) -> i64 {
    sqlx::query_scalar(&format!(
        "SELECT count(*) FROM pg_stat_activity \
         WHERE datname = current_database() AND pid <> pg_backend_pid() AND {predicate}"
    ))
    .fetch_one(store.pool())
    .await
    .expect("read pg_stat_activity")
}

/// Waits, observing rather than sleeping, until `predicate` matches a backend.
async fn wait_for_backend(store: &PostgresStore, predicate: &str, what: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while backends_where(store, predicate).await == 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "{what} never happened"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// An append to a `CommitOrderSlow-*` aggregate blocks AFTER its row (and so
/// its global_nonce) exists, until the test releases `GATE_KEY`: a slow
/// commit whose length the test controls instead of a timer.
async fn install_slow_commit_trigger(store: &PostgresStore) {
    sqlx::query(&format!(
        r#"
        CREATE OR REPLACE FUNCTION commit_order_test_slow_commit() RETURNS trigger AS $$
        BEGIN
            IF NEW.aggregate_id LIKE 'CommitOrderSlow-%' THEN
                PERFORM pg_advisory_xact_lock({GATE_KEY});
            END IF;
            RETURN NEW;
        END
        $$ LANGUAGE plpgsql
        "#
    ))
    .execute(store.pool())
    .await
    .expect("create slow-commit function");
    sqlx::query("DROP TRIGGER IF EXISTS trg_commit_order_test_slow ON events")
        .execute(store.pool())
        .await
        .expect("drop slow-commit trigger");
    sqlx::query(
        "CREATE TRIGGER trg_commit_order_test_slow AFTER INSERT ON events \
         FOR EACH ROW EXECUTE FUNCTION commit_order_test_slow_commit()",
    )
    .execute(store.pool())
    .await
    .expect("create slow-commit trigger");
}

async fn next_event(
    stream: &mut eventstore_core::StoreStream<proto::SubscribeResponse>,
    deadline: tokio::time::Instant,
) -> Option<(String, u64)> {
    loop {
        match tokio::time::timeout_at(deadline, stream.next()).await {
            Err(_) | Ok(None) => return None,
            Ok(Some(Err(e))) => panic!("subscribe error: {e:?}"),
            Ok(Some(Ok(resp))) => {
                if let Some(ev) = resp.event {
                    let meta = ev.meta.expect("meta");
                    return Some((meta.aggregate_id, meta.global_nonce));
                }
            }
        }
    }
}

#[tokio::test]
async fn live_subscriber_receives_an_event_whose_append_commits_after_a_later_nonce() {
    let url = common::get_test_database_url().await;
    let store = PostgresStore::connect_for_tests(&url)
        .await
        .expect("connect");
    install_slow_commit_trigger(&store).await;

    let run = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let slow_id = format!("{SLOW_PREFIX}{run}");
    let fast_id = format!("CommitOrderFast-{run}");

    // Subscribe from the current head so only this test's events arrive.
    let head = store
        .read_all(proto::ReadAllRequest {
            tenant_id: TENANT.into(),
            from_global_nonce: u64::MAX >> 1,
            max_count: 1,
            forward: false,
        })
        .await
        .expect("read head")
        .events
        .first()
        .and_then(|e| e.meta.as_ref().map(|m| m.global_nonce))
        .unwrap_or(0);
    let mut stream = store.subscribe(proto::SubscribeRequest {
        tenant_id: TENANT.into(),
        aggregate_id_prefix: String::new(),
        from_global_nonce: head + 1,
    });

    // Drain replay until the caught-up marker, so the subscriber is live.
    loop {
        match tokio::time::timeout(Duration::from_secs(2), stream.next()).await {
            Ok(Some(Ok(resp))) if resp.event.is_none() => break,
            Ok(Some(Ok(_))) => continue,
            other => panic!("subscriber did not reach live: {other:?}"),
        }
    }

    // Close the gate on a connection of its own.
    let mut gate = store.pool().acquire().await.expect("gate connection");
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(GATE_KEY)
        .execute(&mut *gate)
        .await
        .expect("close gate");

    // Slow append draws nonce N, then blocks in its trigger with N uncommitted.
    let slow_store = store.clone();
    let slow_req = append_request(&slow_id, "Started");
    let slow = tokio::spawn(async move { slow_store.append(slow_req).await });
    wait_for_backend(
        &store,
        "wait_event = 'advisory' AND query LIKE '%INSERT INTO events%'",
        "the slow append blocking in its trigger",
    )
    .await;

    // Fast append for another aggregate. On main it draws N+1 and commits
    // while N is open. With ordered commits it must wait for the slow one.
    let fast_store = store.clone();
    let fast_req = append_request(&fast_id, "Other");
    let mut fast = tokio::spawn(async move { fast_store.append(fast_req).await });
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut seen: Vec<(String, u64)> = Vec::new();
    loop {
        if fast.is_finished() {
            // N+1 committed first: let the live subscriber act on it BEFORE N
            // commits, which is the moment #1545 happened in production.
            if let Some(ev) = next_event(&mut stream, deadline).await {
                seen.push(ev);
            }
            break;
        }
        if backends_where(
            &store,
            "wait_event = 'advisory' AND query LIKE '%pg_advisory_xact_lock%'",
        )
        .await
            > 0
        {
            // The fast append is held behind the slow one's commit. (Another
            // test's appender waiting on its own tenant can match too; then
            // the gate opens early, which only makes this pass where the
            // ordering holds anyway: on main nothing waits on that lock.)
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the fast append neither committed nor waited"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    // Open the gate: N commits now.
    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(GATE_KEY)
        .execute(&mut *gate)
        .await
        .expect("open gate");
    let slow = slow.await.expect("join").expect("slow append");
    let fast = (&mut fast).await.expect("join").expect("fast append");
    assert!(
        slow.last_global_nonce < fast.last_global_nonce,
        "precondition: the slow append must have taken the lower nonce \
         (slow={}, fast={})",
        slow.last_global_nonce,
        fast.last_global_nonce
    );

    // Past the 5s fallback poll, so a skipped event has every chance to show.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(7);
    while seen.len() < 2 {
        let Some(ev) = next_event(&mut stream, deadline).await else {
            break;
        };
        seen.push(ev);
    }

    let ids: Vec<&str> = seen.iter().map(|(id, _)| id.as_str()).collect();
    assert!(
        ids.contains(&slow_id.as_str()),
        "the subscriber never delivered the event at global nonce {} although it is \
         committed; it saw only {seen:?} (syntropic137#1545)",
        slow.last_global_nonce
    );
    let nonces: Vec<u64> = seen.iter().map(|(_, n)| *n).collect();
    assert!(
        nonces.windows(2).all(|w| w[0] < w[1]),
        "events must arrive in global_nonce order, got {seen:?}"
    );
}

/// No trigger, no sleeps: many appenders on distinct aggregates race for
/// nonces while a live subscriber reads. Every committed event must arrive,
/// in strictly increasing global_nonce order. Without ordered commits this
/// loses events intermittently; the deterministic test above pins the
/// mechanism, this one guards the guarantee under real contention.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn live_subscriber_receives_every_event_under_concurrent_appends() {
    const WRITERS: usize = 16;
    const PER_WRITER: u64 = 25;

    let url = common::get_test_database_url().await;
    let store = PostgresStore::connect_for_tests(&url)
        .await
        .expect("connect");

    let run = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    // A tenant of its own, so the subscription sees only this test. Start at
    // the database-wide head so the live polls do not walk other tests' rows.
    let tenant = format!("tenant-commit-order-stress-{run}");
    let head: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(global_nonce), 0) FROM events")
        .fetch_one(store.pool())
        .await
        .expect("read head");

    let mut stream = store.subscribe(proto::SubscribeRequest {
        tenant_id: tenant.clone(),
        aggregate_id_prefix: String::new(),
        from_global_nonce: head as u64 + 1,
    });
    loop {
        match tokio::time::timeout(Duration::from_secs(2), stream.next()).await {
            Ok(Some(Ok(resp))) if resp.event.is_none() => break,
            Ok(Some(Ok(_))) => continue,
            other => panic!("subscriber did not reach live: {other:?}"),
        }
    }

    let mut writers = Vec::with_capacity(WRITERS);
    for w in 0..WRITERS {
        let store = store.clone();
        let tenant = tenant.clone();
        writers.push(tokio::spawn(async move {
            let aggregate_id = format!("Stress-{w}");
            let mut nonces = Vec::with_capacity(PER_WRITER as usize);
            for n in 1..=PER_WRITER {
                let mut req = append_request(&aggregate_id, &format!("E{n}"));
                req.tenant_id = tenant.clone();
                req.expected_aggregate_nonce = n - 1;
                let ev = req.events[0].meta.as_mut().unwrap();
                ev.tenant_id = tenant.clone();
                ev.aggregate_nonce = n;
                let resp = store.append(req).await.expect("append");
                nonces.push(resp.last_global_nonce);
            }
            nonces
        }));
    }

    let total = WRITERS * PER_WRITER as usize;
    let mut seen: Vec<u64> = Vec::with_capacity(total);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while seen.len() < total {
        let Ok(next) = tokio::time::timeout_at(deadline, stream.next()).await else {
            break;
        };
        match next {
            Some(Ok(resp)) => {
                if let Some(ev) = resp.event {
                    seen.push(ev.meta.expect("meta").global_nonce);
                }
            }
            Some(Err(e)) => panic!("subscribe error: {e:?}"),
            None => break,
        }
    }

    let mut committed: Vec<u64> = Vec::with_capacity(total);
    for w in writers {
        committed.extend(w.await.expect("join"));
    }
    committed.sort_unstable();

    let missing: Vec<u64> = committed
        .iter()
        .copied()
        .filter(|n| !seen.contains(n))
        .collect();
    assert!(
        missing.is_empty(),
        "the live subscriber skipped {} committed event(s) at global nonces {missing:?} \
         (syntropic137#1545)",
        missing.len()
    );
    assert!(
        seen.windows(2).all(|w| w[0] < w[1]),
        "events must arrive in strictly increasing global_nonce order"
    );
}

/// Append throughput on ONE tenant (the worst case for a per-tenant ordering
/// lock), through the production pool (`connect`, 5 connections). Ignored by
/// default; run with:
/// `cargo test -p eventstore-backend-postgres --features test-utils --release \
///   --test it_commit_order -- --ignored --nocapture append_throughput`
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore]
async fn append_throughput() {
    const TOTAL: u64 = 4000;
    let url = common::get_test_database_url().await;
    let store = PostgresStore::connect(&url).await.expect("connect");

    for writers in [1u64, 4, 16, 64] {
        let run = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let tenant = format!("tenant-bench-{run}");
        let per_writer = TOTAL / writers;
        let started = std::time::Instant::now();
        let mut tasks = Vec::new();
        for w in 0..writers {
            let store = store.clone();
            let tenant = tenant.clone();
            tasks.push(tokio::spawn(async move {
                let aggregate_id = format!("Bench-{w}");
                for n in 1..=per_writer {
                    let mut req = append_request(&aggregate_id, &format!("E{n}"));
                    req.tenant_id = tenant.clone();
                    req.expected_aggregate_nonce = n - 1;
                    let ev = req.events[0].meta.as_mut().unwrap();
                    ev.tenant_id = tenant.clone();
                    ev.aggregate_nonce = n;
                    store.append(req).await.expect("append");
                }
            }));
        }
        for t in tasks {
            t.await.expect("join");
        }
        let secs = started.elapsed().as_secs_f64();
        let n = per_writer * writers;
        println!(
            "append_throughput writers={writers:>2} appends={n} secs={secs:.2} appends_per_sec={:.0}",
            n as f64 / secs
        );
    }
}
