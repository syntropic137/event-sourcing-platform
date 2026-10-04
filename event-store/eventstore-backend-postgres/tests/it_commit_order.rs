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

/// Holds any append to a `CommitOrderSlow-*` aggregate open for 600ms AFTER
/// its row (and so its global_nonce) exists, standing in for a slow commit.
async fn install_slow_commit_trigger(store: &PostgresStore) {
    sqlx::query(
        r#"
        CREATE OR REPLACE FUNCTION commit_order_test_slow_commit() RETURNS trigger AS $$
        BEGIN
            IF NEW.aggregate_id LIKE 'CommitOrderSlow-%' THEN
                PERFORM pg_sleep(0.6);
            END IF;
            RETURN NEW;
        END
        $$ LANGUAGE plpgsql
        "#,
    )
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

    // Slow append takes nonce N and holds its transaction open.
    let slow_store = store.clone();
    let slow_req = append_request(&slow_id, "Started");
    let slow = tokio::spawn(async move { slow_store.append(slow_req).await });
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Fast append for another aggregate: nonce N+1, committed while N is not.
    let fast = store
        .append(append_request(&fast_id, "Other"))
        .await
        .expect("fast append");

    // Read the subscription WHILE the slow append is still open - this is a
    // live subscriber, it polls when woken, not when the test is ready.
    let mut seen: Vec<(String, u64)> = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    while seen.len() < 2 {
        let Ok(next) = tokio::time::timeout_at(deadline, stream.next()).await else {
            break;
        };
        match next {
            Some(Ok(resp)) => {
                if let Some(ev) = resp.event {
                    let meta = ev.meta.expect("meta");
                    seen.push((meta.aggregate_id, meta.global_nonce));
                }
            }
            Some(Err(e)) => panic!("subscribe error: {e:?}"),
            None => break,
        }
    }

    let slow = slow.await.expect("join").expect("slow append");
    assert!(
        slow.last_global_nonce < fast.last_global_nonce,
        "precondition: the slow append must have taken the lower nonce \
         (slow={}, fast={})",
        slow.last_global_nonce,
        fast.last_global_nonce
    );

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
    // A tenant of its own, so the subscription from 0 sees only this test.
    let tenant = format!("tenant-commit-order-stress-{run}");

    let mut stream = store.subscribe(proto::SubscribeRequest {
        tenant_id: tenant.clone(),
        aggregate_id_prefix: String::new(),
        from_global_nonce: 0,
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
