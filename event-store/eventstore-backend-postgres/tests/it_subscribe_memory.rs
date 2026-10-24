//! Concurrent replay subscriptions stay within a fixed memory bound
//! (syntropic137#1553, fixed by #369).
//!
//! Before #369, every `subscribe()` loaded the whole history after its
//! cursor into memory before yielding the first event, so N subscribers
//! catching up together held N copies of the history. In syntropic137 that
//! got the event store OOM-killed in a loop: each restart reconnected every
//! projection, and they all replayed at once.
//!
//! `it_subscribe_paging` pins the mechanism (pages are fetched lazily, one
//! at a time). This test pins the outcome the incident was about: 8
//! subscribers replaying 50k events of 1 KiB each at the same time, with the
//! production page size, must not grow the process's peak RSS by more than
//! 64 MiB. Measured on a debug build: about 35-39 MiB with paging, and
//! about 1.2 GiB with the page bound removed or on the parent of #369.
//!
//! Half the subscribers filter by a prefix every aggregate matches, so the
//! same history crosses both query arms: dropping the page bound from
//! either one alone still breaks the budget (#455).
//!
//! Peak RSS comes from `/proc/self/status` (`VmHWM`), so the test is
//! Linux-only. It is the only test in this binary, so no other test's
//! allocations count against it. Postgres runs out of process.

#![cfg(target_os = "linux")]

mod common;

use std::sync::Arc;
use std::time::Duration;

use eventstore_backend_postgres::store_postgres::DEFAULT_SUBSCRIBE_PAGE_SIZE;
use eventstore_backend_postgres::PostgresStore;
use eventstore_core::{proto, EventStore};
use futures::StreamExt;
use tokio::sync::Barrier;

const SUBSCRIBERS: usize = 8;
/// Matches every aggregate `write_history` writes.
const PREFIX: &str = "Order-";
const AGGREGATES: u64 = 50;
const EVENTS_PER_AGGREGATE: u64 = 1_000;
const TOTAL_EVENTS: usize = (AGGREGATES * EVENTS_PER_AGGREGATE) as usize;
const PAYLOAD_BYTES: usize = 1024;
const RSS_BOUND_BYTES: u64 = 64 * 1024 * 1024;
const STEP: Duration = Duration::from_secs(60);

fn unique_tenant() -> String {
    let run = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("tenant-memory-{run}")
}

fn event(tenant: &str, aggregate_id: &str, nonce: u64) -> proto::EventData {
    proto::EventData {
        meta: Some(proto::EventMetadata {
            event_id: format!("{tenant}-{aggregate_id}-{nonce}"),
            aggregate_id: aggregate_id.into(),
            aggregate_type: "Order".into(),
            aggregate_nonce: nonce,
            event_type: "Happened".into(),
            event_version: 1,
            content_type: "application/octet-stream".into(),
            tenant_id: tenant.into(),
            ..Default::default()
        }),
        payload: vec![nonce as u8; PAYLOAD_BYTES],
    }
}

/// Append the whole history, one request per aggregate.
async fn write_history(store: &PostgresStore, tenant: &str) {
    for a in 0..AGGREGATES {
        let aggregate_id = format!("{PREFIX}{a}");
        store
            .append(proto::AppendRequest {
                tenant_id: tenant.into(),
                aggregate_id: aggregate_id.clone(),
                aggregate_type: "Order".into(),
                expected_aggregate_nonce: 0,
                idempotency_key: String::new(),
                events: (1..=EVENTS_PER_AGGREGATE)
                    .map(|n| event(tenant, &aggregate_id, n))
                    .collect(),
            })
            .await
            .expect("append");
    }
}

/// A `kB` field of `/proc/self/status`, in bytes.
fn proc_status_bytes(field: &str) -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").expect("read /proc/self/status");
    let line = status
        .lines()
        .find(|l| l.starts_with(field))
        .unwrap_or_else(|| panic!("{field} missing from /proc/self/status"));
    let kib: u64 = line
        .split_whitespace()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| panic!("unparseable line: {line}"));
    kib * 1024
}

/// Reset the peak RSS to the current RSS and return it.
fn reset_peak_rss() -> u64 {
    std::fs::write("/proc/self/clear_refs", "5").expect("reset VmHWM via /proc/self/clear_refs");
    proc_status_bytes("VmRSS:")
}

/// Replay `prefix` from 0 to the caught-up marker. Waits at `barrier` after the
/// first event, so all subscribers are mid-replay at the same time.
async fn replay(
    store: Arc<PostgresStore>,
    tenant: String,
    prefix: &'static str,
    barrier: Arc<Barrier>,
) -> usize {
    let mut stream = store.subscribe(proto::SubscribeRequest {
        tenant_id: tenant,
        aggregate_id_prefix: prefix.into(),
        from_global_nonce: 0,
    });
    let mut delivered = 0;
    loop {
        let item = tokio::time::timeout(STEP, stream.next())
            .await
            .expect("subscription produced nothing within the step timeout");
        match item {
            Some(Ok(proto::SubscribeResponse { event: Some(_) })) => {
                delivered += 1;
                if delivered == 1 {
                    barrier.wait().await;
                }
            }
            Some(Ok(proto::SubscribeResponse { event: None })) => return delivered,
            other => panic!("expected events then a caught-up marker, got {other:?}"),
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrent_replays_stay_within_the_rss_bound() {
    let url = common::get_test_database_url().await;
    let store = PostgresStore::connect_for_tests(&url)
        .await
        .expect("connect");
    assert_eq!(store.subscribe_page_size(), DEFAULT_SUBSCRIBE_PAGE_SIZE);
    let tenant = unique_tenant();
    // Written before the baseline: the write path's own allocations are not
    // what this test bounds.
    write_history(&store, &tenant).await;

    let baseline = reset_peak_rss();
    let barrier = Arc::new(Barrier::new(SUBSCRIBERS));
    let subscribers: Vec<_> = (0..SUBSCRIBERS)
        .map(|i| {
            let prefix = if i % 2 == 0 { "" } else { PREFIX };
            tokio::spawn(replay(
                store.clone(),
                tenant.clone(),
                prefix,
                barrier.clone(),
            ))
        })
        .collect();
    for s in subscribers {
        assert_eq!(s.await.expect("join"), TOTAL_EVENTS, "every event replayed");
    }
    let growth = proc_status_bytes("VmHWM:").saturating_sub(baseline);

    println!(
        "{SUBSCRIBERS} subscribers x {TOTAL_EVENTS} events x {PAYLOAD_BYTES} B: \
         peak RSS grew {} KiB (bound {} KiB)",
        growth / 1024,
        RSS_BOUND_BYTES / 1024
    );
    assert!(
        growth <= RSS_BOUND_BYTES,
        "peak RSS grew {} MiB replaying to {SUBSCRIBERS} subscribers; bound is {} MiB. \
         A subscriber must hold one page, not the history (syntropic137#1553, #369)",
        growth / (1024 * 1024),
        RSS_BOUND_BYTES / (1024 * 1024)
    );
}
