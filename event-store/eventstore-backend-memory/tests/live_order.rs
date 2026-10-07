//! The memory backend advertises `commit_ordered_global_nonce`. These tests
//! check the behavior behind the flag, not just the flag: a live subscriber
//! sees every global nonce exactly once, in strictly increasing order, under
//! many concurrent writers, and a lagging subscriber is told instead of
//! silently skipping events.

use std::sync::Arc;
use std::time::Duration;

use eventstore_backend_memory::InMemoryStore;
use eventstore_core::proto::{AppendRequest, EventData, EventMetadata, SubscribeRequest};
use eventstore_core::{capabilities, EventStore, StoreError};
use tokio_stream::StreamExt;

const TENANT: &str = "tenant-order";

fn event(aggregate_id: &str, nonce: u64) -> EventData {
    EventData {
        meta: Some(EventMetadata {
            event_id: format!("{aggregate_id}-{nonce}"),
            aggregate_id: aggregate_id.into(),
            aggregate_type: "Order".into(),
            aggregate_nonce: nonce,
            event_type: "Tick".into(),
            event_version: 1,
            content_type: "application/octet-stream".into(),
            tenant_id: TENANT.into(),
            ..Default::default()
        }),
        payload: vec![],
    }
}

/// Append `batches` batches of `batch_size` events to one aggregate.
async fn writer(store: Arc<InMemoryStore>, aggregate_id: String, batches: u64, batch_size: u64) {
    let mut head = 0;
    for _ in 0..batches {
        let events = (1..=batch_size)
            .map(|i| event(&aggregate_id, head + i))
            .collect();
        store
            .append(AppendRequest {
                tenant_id: TENANT.into(),
                aggregate_id: aggregate_id.clone(),
                aggregate_type: "Order".into(),
                expected_aggregate_nonce: head,
                idempotency_key: String::new(),
                events,
            })
            .await
            .expect("append");
        head += batch_size;
        tokio::task::yield_now().await;
    }
}

fn subscribe_all(
    store: &InMemoryStore,
) -> eventstore_core::StoreStream<eventstore_core::proto::SubscribeResponse> {
    store.subscribe(SubscribeRequest {
        tenant_id: TENANT.into(),
        aggregate_id_prefix: String::new(),
        from_global_nonce: 0,
    })
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

    // One subscriber attached before any write, one attached mid-flight so
    // the replay/live boundary is crossed while writers are running.
    let early = subscribe_all(&store);

    let writers: Vec<_> = (0..WRITERS)
        .map(|w| tokio::spawn(writer(store.clone(), format!("agg-{w}"), BATCHES, BATCH)))
        .collect();

    tokio::time::sleep(Duration::from_millis(2)).await;
    let late = subscribe_all(&store);

    let collect = |stream: eventstore_core::StoreStream<_>| {
        tokio::spawn(async move {
            let mut stream = stream;
            let mut seen = Vec::with_capacity(TOTAL as usize);
            while (seen.len() as u64) < TOTAL {
                let item = tokio::time::timeout(Duration::from_secs(10), stream.next())
                    .await
                    .expect("timed out waiting for live event")
                    .expect("stream ended early");
                let resp: eventstore_core::proto::SubscribeResponse =
                    item.expect("subscription error");
                if let Some(ev) = resp.event {
                    seen.push(ev.meta.unwrap().global_nonce);
                }
            }
            seen
        })
    };
    let early = collect(early);
    let late = collect(late);

    for w in writers {
        w.await.unwrap();
    }

    let expected: Vec<u64> = (1..=TOTAL).collect();
    for (name, handle) in [("early", early), ("late", late)] {
        let seen = handle.await.unwrap();
        if let Some(pos) = seen.windows(2).position(|w| w[1] <= w[0]) {
            panic!(
                "{name} subscriber: nonce {} delivered after {} (index {pos})",
                seen[pos + 1],
                seen[pos]
            );
        }
        assert_eq!(
            seen, expected,
            "{name} subscriber missed or duplicated nonces"
        );
    }
}

#[tokio::test]
async fn lagging_subscriber_gets_an_error_not_a_silent_gap() {
    let store = InMemoryStore::with_broadcast_capacity(4);
    let mut sub = subscribe_all(&store);

    // Write more than the buffer holds before the subscriber reads anything.
    for n in 1..=10 {
        store
            .append(AppendRequest {
                tenant_id: TENANT.into(),
                aggregate_id: "agg-lag".into(),
                aggregate_type: "Order".into(),
                expected_aggregate_nonce: n - 1,
                idempotency_key: String::new(),
                events: vec![event("agg-lag", n)],
            })
            .await
            .unwrap();
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
}
