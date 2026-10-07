//! A subscription must surface database failures, never hide them (#350).
//!
//! Before the fix, a failed replay or live query was turned into an empty
//! result set. A failed replay then looked like a successful catch-up (the
//! caught-up marker was sent) and a failed live poll looked like "no new
//! events". These tests inject a database fault into the subscriber's own
//! connection pool (closing it), in the replay and in the live phase, with
//! and without an aggregate id prefix. They check that:
//!
//! - the stream yields a typed `UNAVAILABLE` error and then ends,
//! - no caught-up marker is sent while replay is failing,
//! - reconnecting from the consumer's saved checkpoint delivers every
//!   committed event, including those appended during the outage.

mod common;

use std::sync::Arc;
use std::time::Duration;

use eventstore_backend_postgres::PostgresStore;
use eventstore_core::{proto, EventStore, StoreError, StoreStream};
use futures::StreamExt;
use tonic::Code;

const PREFIX: &str = "Order-";
const STEP: Duration = Duration::from_secs(15);

fn unique_tenant(name: &str) -> String {
    let run = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("tenant-subfault-{name}-{run}")
}

async fn connect() -> Arc<PostgresStore> {
    let url = common::get_test_database_url().await;
    PostgresStore::connect_for_tests(&url)
        .await
        .expect("connect")
}

/// Append `count` events to `aggregate_id`, continuing after `after_nonce`.
async fn append(
    store: &PostgresStore,
    tenant: &str,
    aggregate_id: &str,
    after_nonce: u64,
    count: u64,
) -> Vec<u64> {
    let events = (1..=count)
        .map(|i| {
            let nonce = after_nonce + i;
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
                payload: vec![nonce as u8],
            }
        })
        .collect();
    let res = store
        .append(proto::AppendRequest {
            tenant_id: tenant.into(),
            aggregate_id: aggregate_id.into(),
            aggregate_type: "Order".into(),
            expected_aggregate_nonce: after_nonce,
            idempotency_key: String::new(),
            events,
        })
        .await
        .expect("append");
    // Nonces of one batch need not be contiguous (other tenants append
    // concurrently), so read them back.
    let read = store
        .read_stream(proto::ReadStreamRequest {
            tenant_id: tenant.into(),
            aggregate_id: aggregate_id.into(),
            from_aggregate_nonce: after_nonce + 1,
            max_count: count as u32,
            forward: true,
        })
        .await
        .expect("read back");
    let nonces: Vec<u64> = read
        .events
        .iter()
        .map(|e| e.meta.as_ref().expect("meta").global_nonce)
        .collect();
    assert_eq!(nonces.last().copied(), Some(res.last_global_nonce));
    nonces
}

fn request(tenant: &str, prefix: &str, from: u64) -> proto::SubscribeRequest {
    proto::SubscribeRequest {
        tenant_id: tenant.into(),
        aggregate_id_prefix: prefix.into(),
        from_global_nonce: from,
    }
}

async fn next(
    stream: &mut StoreStream<proto::SubscribeResponse>,
) -> Option<Result<proto::SubscribeResponse, StoreError>> {
    tokio::time::timeout(STEP, stream.next())
        .await
        .expect("subscription produced nothing within the step timeout")
}

/// Collect delivered global nonces until the first caught-up marker.
async fn collect_until_caught_up(stream: &mut StoreStream<proto::SubscribeResponse>) -> Vec<u64> {
    let mut seen = Vec::new();
    loop {
        match next(stream).await {
            Some(Ok(resp)) => match resp.event {
                Some(ev) => seen.push(ev.meta.expect("meta").global_nonce),
                None => return seen,
            },
            other => panic!("expected events then a caught-up marker, got {other:?}"),
        }
    }
}

/// Next delivered event, skipping keep-alive markers.
async fn next_event(stream: &mut StoreStream<proto::SubscribeResponse>) -> u64 {
    loop {
        match next(stream).await {
            Some(Ok(resp)) => {
                if let Some(ev) = resp.event {
                    return ev.meta.expect("meta").global_nonce;
                }
            }
            other => panic!("expected an event, got {other:?}"),
        }
    }
}

/// The stream must yield a typed UNAVAILABLE error (not an empty result and
/// not a caught-up marker), then end.
async fn expect_unavailable_then_end(
    stream: &mut StoreStream<proto::SubscribeResponse>,
    resume_from: u64,
) {
    match next(stream).await {
        Some(Err(e)) => {
            assert_eq!(
                e.to_status().code(),
                Code::Unavailable,
                "subscription DB failure must map to UNAVAILABLE, got {e:?}"
            );
            let resume = format!("resume from global_nonce {resume_from} ");
            assert!(
                e.to_string().contains(&resume),
                "error should report the last delivered cursor + 1 ({resume_from}): {e}"
            );
        }
        Some(Ok(resp)) if resp.event.is_none() => {
            panic!("a failing subscription must not report caught-up / keep-alive")
        }
        Some(Ok(resp)) => panic!("unexpected event while the pool is closed: {resp:?}"),
        None => panic!("stream ended without surfacing the database error"),
    }
    assert!(
        next(stream).await.is_none(),
        "stream must end after surfacing the error"
    );
}

async fn replay_failure_is_surfaced_and_recoverable(prefix: &str) {
    let tenant = unique_tenant(if prefix.is_empty() {
        "replay"
    } else {
        "replay-prefix"
    });
    let writer = connect().await;
    let mut expected = append(&writer, &tenant, "Order-1", 0, 3).await;
    let other = append(&writer, &tenant, "Other-1", 0, 1).await;
    if prefix.is_empty() {
        expected.extend(other);
    }

    // Fault: the subscriber's pool is gone before the replay query runs.
    let subscriber = connect().await;
    // This is the behavior the backend advertises as a capability.
    assert!(subscriber
        .capabilities()
        .contains(&eventstore_core::capabilities::SUBSCRIPTION_ERRORS_SURFACED));
    subscriber.pool().close().await;
    let mut stream = subscriber.subscribe(request(&tenant, prefix, 0));
    expect_unavailable_then_end(&mut stream, 0).await;

    // Reconnect from the saved checkpoint (nothing processed yet).
    let recovered = connect().await;
    let mut stream = recovered.subscribe(request(&tenant, prefix, 0));
    assert_eq!(collect_until_caught_up(&mut stream).await, expected);
}

async fn live_failure_is_surfaced_and_recoverable(prefix: &str) {
    let tenant = unique_tenant(if prefix.is_empty() {
        "live"
    } else {
        "live-prefix"
    });
    let writer = connect().await;
    let initial = append(&writer, &tenant, "Order-1", 0, 2).await;

    let subscriber = connect().await;
    let mut stream = subscriber.subscribe(request(&tenant, prefix, 0));
    assert_eq!(collect_until_caught_up(&mut stream).await, initial);

    // One event delivered live before the fault; the consumer checkpoints it.
    let live = append(&writer, &tenant, "Order-1", 2, 1).await;
    let checkpoint = next_event(&mut stream).await;
    assert_eq!(checkpoint, live[0]);

    // Fault, then commit more events during the outage. The NOTIFY wakes the
    // subscriber (its listener has its own connection); the poll then fails.
    subscriber.pool().close().await;
    let mut missed = append(&writer, &tenant, "Order-1", 3, 2).await;
    let other = append(&writer, &tenant, "Other-1", 0, 1).await;
    if prefix.is_empty() {
        missed.extend(other);
    }
    expect_unavailable_then_end(&mut stream, checkpoint + 1).await;

    // Reconnect from the consumer's saved position: nothing committed is lost.
    let recovered = connect().await;
    let mut stream = recovered.subscribe(request(&tenant, prefix, checkpoint + 1));
    assert_eq!(collect_until_caught_up(&mut stream).await, missed);
}

#[tokio::test]
async fn replay_query_failure_surfaces_unavailable_without_prefix() {
    replay_failure_is_surfaced_and_recoverable("").await;
}

#[tokio::test]
async fn replay_query_failure_surfaces_unavailable_with_prefix() {
    replay_failure_is_surfaced_and_recoverable(PREFIX).await;
}

#[tokio::test]
async fn live_query_failure_surfaces_unavailable_without_prefix() {
    live_failure_is_surfaced_and_recoverable("").await;
}

#[tokio::test]
async fn live_query_failure_surfaces_unavailable_with_prefix() {
    live_failure_is_surfaced_and_recoverable(PREFIX).await;
}
