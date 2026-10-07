//! The memory backend advertises `commit_ordered_global_nonce`. These tests
//! check the behavior behind the flag, not just the flag: a live subscriber
//! sees every matching global nonce exactly once, in strictly increasing
//! order, under many concurrent writers (including across the replay/live
//! handoff and with tenant/prefix filters), and a lagging subscriber is told
//! instead of silently skipping events.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use eventstore_backend_memory::InMemoryStore;
use eventstore_core::proto::{
    AppendRequest, EventData, EventMetadata, SubscribeRequest, SubscribeResponse,
};
use eventstore_core::{capabilities, EventStore, StoreError, StoreStream};
use tokio::sync::{watch, Barrier};
use tokio_stream::StreamExt;

const TENANT: &str = "tenant-order";

fn event(tenant: &str, aggregate_id: &str, nonce: u64) -> EventData {
    EventData {
        meta: Some(EventMetadata {
            event_id: format!("{tenant}-{aggregate_id}-{nonce}"),
            aggregate_id: aggregate_id.into(),
            aggregate_type: "Order".into(),
            aggregate_nonce: nonce,
            event_type: "Tick".into(),
            event_version: 1,
            content_type: "application/octet-stream".into(),
            tenant_id: tenant.into(),
            ..Default::default()
        }),
        payload: vec![],
    }
}

/// Append one batch; return the global nonces it was assigned.
async fn append_batch(
    store: &InMemoryStore,
    tenant: &str,
    aggregate_id: &str,
    head: u64,
    size: u64,
) -> Vec<u64> {
    let events = (1..=size)
        .map(|i| event(tenant, aggregate_id, head + i))
        .collect();
    let resp = store
        .append(AppendRequest {
            tenant_id: tenant.into(),
            aggregate_id: aggregate_id.into(),
            aggregate_type: "Order".into(),
            expected_aggregate_nonce: head,
            idempotency_key: String::new(),
            events,
        })
        .await
        .expect("append");
    (resp.last_global_nonce + 1 - size..=resp.last_global_nonce).collect()
}

fn subscribe(store: &InMemoryStore, tenant: &str, prefix: &str) -> StoreStream<SubscribeResponse> {
    store.subscribe(SubscribeRequest {
        tenant_id: tenant.into(),
        aggregate_id_prefix: prefix.into(),
        from_global_nonce: 0,
    })
}

/// Collect `count` global nonces from a subscription on a separate task.
fn collect(
    stream: StoreStream<SubscribeResponse>,
    count: usize,
) -> tokio::task::JoinHandle<Vec<u64>> {
    tokio::spawn(async move {
        let mut stream = stream;
        let mut seen = Vec::with_capacity(count);
        while seen.len() < count {
            let item = tokio::time::timeout(Duration::from_secs(10), stream.next())
                .await
                .expect("timed out waiting for event")
                .expect("stream ended early");
            if let Some(ev) = item.expect("subscription error").event {
                seen.push(ev.meta.unwrap().global_nonce);
            }
        }
        seen
    })
}

fn assert_exact_order(name: &str, seen: &[u64], expected: &[u64]) {
    if let Some(pos) = seen.windows(2).position(|w| w[1] <= w[0]) {
        panic!(
            "{name}: nonce {} delivered after {} (index {pos})",
            seen[pos + 1],
            seen[pos]
        );
    }
    assert_eq!(seen, expected, "{name}: missed or duplicated nonces");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrent_appends_reach_live_subscriber_in_global_nonce_order() {
    const WRITERS: u64 = 32;
    const BATCHES: u64 = 50;
    const BATCH: u64 = 3;
    const TOTAL: u64 = WRITERS * BATCHES * BATCH; // 4800 events

    // Large buffer: this test is about ordering, not lag (tested below).
    let store = InMemoryStore::with_broadcast_capacity(TOTAL as usize);
    assert!(store
        .capabilities()
        .contains(&capabilities::COMMIT_ORDERED_GLOBAL_NONCE));

    // `early` attaches before any write. `late` attaches while writers are
    // mid-flight: each writer does half its batches, meets the barrier, keeps
    // writing, and holds its final batch until `late` is attached. So the
    // replay/live handoff is always crossed with appends in progress.
    let early = collect(subscribe(&store, TENANT, ""), TOTAL as usize);
    let barrier = Arc::new(Barrier::new(WRITERS as usize + 1));
    let (release_tx, release_rx) = watch::channel(false);
    let appended = Arc::new(AtomicU64::new(0));

    let writers: Vec<_> = (0..WRITERS)
        .map(|w| {
            let (store, barrier, appended) = (store.clone(), barrier.clone(), appended.clone());
            let mut release = release_rx.clone();
            tokio::spawn(async move {
                let agg = format!("agg-{w}");
                for b in 0..BATCHES {
                    if b == BATCHES / 2 {
                        barrier.wait().await;
                    }
                    if b == BATCHES - 1 {
                        release.wait_for(|go| *go).await.unwrap();
                    }
                    append_batch(&store, TENANT, &agg, b * BATCH, BATCH).await;
                    appended.fetch_add(BATCH, Ordering::SeqCst);
                    tokio::task::yield_now().await;
                }
            })
        })
        .collect();

    barrier.wait().await;
    let late = collect(subscribe(&store, TENANT, ""), TOTAL as usize);
    let at_attach = appended.load(Ordering::SeqCst);
    assert!(
        at_attach < TOTAL,
        "late subscriber must attach before all writes finish"
    );
    release_tx.send(true).unwrap();

    for w in writers {
        w.await.unwrap();
    }

    let expected: Vec<u64> = (1..=TOTAL).collect();
    assert_exact_order("early", &early.await.unwrap(), &expected);
    assert_exact_order("late", &late.await.unwrap(), &expected);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn filtered_live_subscriber_sees_only_its_events_in_order() {
    const WRITERS: u64 = 16;
    const BATCHES: u64 = 40;
    const BATCH: u64 = 2;

    let store = InMemoryStore::with_broadcast_capacity(8192);

    // Interleave four kinds of writer: two tenants x matching/non-matching
    // prefix. Only TENANT + "keep-" should reach the subscriber.
    let kinds = [
        (TENANT, "keep-"),
        (TENANT, "drop-"),
        ("tenant-other", "keep-"),
        ("tenant-other", "drop-"),
    ];
    let matching_writers = WRITERS / kinds.len() as u64;
    let expected_count = (matching_writers * BATCHES * BATCH) as usize;
    let sub = collect(subscribe(&store, TENANT, "keep-"), expected_count);

    let writers: Vec<_> = (0..WRITERS)
        .map(|w| {
            let store = store.clone();
            let (tenant, prefix) = kinds[(w % kinds.len() as u64) as usize];
            tokio::spawn(async move {
                let agg = format!("{prefix}{w}");
                let mut nonces = Vec::new();
                for b in 0..BATCHES {
                    nonces.extend(append_batch(&store, tenant, &agg, b * BATCH, BATCH).await);
                    tokio::task::yield_now().await;
                }
                (tenant == TENANT && prefix == "keep-", nonces)
            })
        })
        .collect();

    let mut expected = Vec::new();
    for w in writers {
        let (matches, nonces) = w.await.unwrap();
        if matches {
            expected.extend(nonces);
        }
    }
    expected.sort_unstable();
    assert_eq!(expected.len(), expected_count);
    assert_exact_order("filtered", &sub.await.unwrap(), &expected);
}

#[tokio::test]
async fn store_shutdown_ends_live_subscription_with_an_error() {
    let store = InMemoryStore::new();
    append_batch(&store, TENANT, "agg-close", 0, 1).await;
    let mut sub = subscribe(&store, TENANT, "");
    drop(store); // last owner: the broadcast channel closes

    let replayed = sub.next().await.expect("replayed event").expect("ok");
    assert_eq!(replayed.event.unwrap().meta.unwrap().global_nonce, 1);
    match tokio::time::timeout(Duration::from_secs(5), sub.next())
        .await
        .expect("timeout")
    {
        Some(Err(StoreError::Unavailable(msg))) => assert!(msg.contains("closed"), "{msg}"),
        other => panic!("closure must surface as an error, got {other:?}"),
    }
    assert!(sub.next().await.is_none(), "stream ends after the error");
}

#[tokio::test]
async fn lagging_subscriber_gets_an_error_then_resubscribes_from_checkpoint() {
    let store = InMemoryStore::with_broadcast_capacity(4);
    // Lag is the memory backend's only way to stop delivering; surfacing it
    // is what backs its subscription_errors_surfaced flag.
    assert!(store
        .capabilities()
        .contains(&capabilities::SUBSCRIPTION_ERRORS_SURFACED));
    let mut sub = subscribe(&store, TENANT, "");

    // Write more than the buffer holds before the subscriber reads anything.
    for n in 0..10 {
        append_batch(&store, TENANT, "agg-lag", n, 1).await;
    }

    let first = tokio::time::timeout(Duration::from_secs(5), sub.next())
        .await
        .expect("timeout")
        .expect("stream item");
    match first {
        Err(StoreError::ResourceExhausted(msg)) => assert!(msg.contains("lagged"), "{msg}"),
        Err(other) => panic!("unexpected error: {other}"),
        Ok(resp) => panic!(
            "lagged subscriber silently skipped to {:?}",
            resp.event.and_then(|e| e.meta).map(|m| m.global_nonce)
        ),
    }
    let after = tokio::time::timeout(Duration::from_secs(5), sub.next())
        .await
        .expect("timeout");
    assert!(after.is_none(), "stream must end after the lag error");

    // The consumer's checkpoint is "nothing delivered": resubscribing from
    // there replays every event, in order.
    let seen = collect(subscribe(&store, TENANT, ""), 10).await.unwrap();
    assert_exact_order("resubscribed", &seen, &(1..=10).collect::<Vec<_>>());
}

/// `\`, `%` and `_` in a prefix are ordinary characters (#361), in replay
/// and in live delivery.
#[tokio::test]
async fn prefix_wildcard_characters_match_literally() {
    let store = InMemoryStore::new();
    assert!(store
        .capabilities()
        .contains(&capabilities::LITERAL_SUBSCRIPTION_PREFIX));
    let tenant = "tenant-literal";
    let ids = [
        "acct_1",
        "acctX1",
        "acct%1",
        "acct-long-1",
        r"acct\1",
        r"acct\\1",
        "acct1",
    ];
    let mut all = Vec::new();
    for id in ids {
        all.push((
            id.to_string(),
            append_batch(&store, tenant, id, 0, 1).await[0],
        ));
    }

    for (i, prefix) in ["acct_", "acct%", r"acct\", r"acct\\", "acct_1"]
        .into_iter()
        .enumerate()
    {
        let expected: Vec<u64> = all
            .iter()
            .filter(|(id, _)| id.starts_with(prefix))
            .map(|(_, g)| *g)
            .collect();
        assert!(!expected.is_empty(), "{prefix}");
        let replay = collect(subscribe(&store, tenant, prefix), expected.len()).await;
        assert_exact_order(prefix, &replay.expect("join"), &expected);

        // Live: one foreign id an unescaped LIKE would match, then a match.
        let mut stream = subscribe(&store, tenant, prefix);
        let mut seen = 0;
        while seen < expected.len() {
            let item = tokio::time::timeout(Duration::from_secs(10), stream.next())
                .await
                .expect("replay")
                .expect("open");
            if item.expect("ok").event.is_some() {
                seen += 1;
            }
        }
        let foreign = format!("{}{i}-live", prefix.replace(['_', '%', '\\'], "Q"));
        let foreign_nonce = append_batch(&store, tenant, &foreign, 0, 1).await[0];
        all.push((foreign, foreign_nonce));
        let own = format!("{prefix}{i}-live");
        let wanted = append_batch(&store, tenant, &own, 0, 1).await[0];
        let got = loop {
            let item = tokio::time::timeout(Duration::from_secs(10), stream.next())
                .await
                .expect("live")
                .expect("open");
            if let Some(ev) = item.expect("ok").event {
                break ev.meta.unwrap();
            }
        };
        assert_eq!(
            (got.aggregate_id.as_str(), got.global_nonce),
            (own.as_str(), wanted),
            "{prefix}"
        );
        all.push((own, wanted));
    }
}
