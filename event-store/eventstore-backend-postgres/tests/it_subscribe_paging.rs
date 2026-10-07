//! Subscription replay and live delivery read in bounded keyset pages
//! (#369).
//!
//! Before #369, `subscribe()` loaded the whole history after the cursor into
//! memory before yielding the first event (1M events: 3.6 s to first event,
//! ~1.9 GB server RSS). These tests use a tiny page size so that every
//! history spans many pages, and check that paging changes nothing a
//! consumer can observe:
//!
//! - every event is delivered exactly once, in `global_nonce` order, across
//!   page boundaries, with and without a prefix, and while appends race the
//!   replay (commit-order visibility, #337);
//! - pages are fetched lazily, one at a time (bounded memory);
//! - the caught-up marker comes once, after the last page;
//! - a query failing mid-pagination ends the stream with `UNAVAILABLE` at
//!   the last delivered position (ADR-026), and reconnecting from the
//!   checkpoint delivers the rest;
//! - an undecodable row inside a page or first in a page ends the stream
//!   with `DATA_LOSS` after exactly the events before it (#351).

mod common;

use std::sync::Arc;
use std::time::Duration;

use eventstore_backend_postgres::PostgresStore;
use eventstore_core::{proto, EventStore, StoreError, StoreStream};
use futures::StreamExt;
use tonic::Code;

const STEP: Duration = Duration::from_secs(15);

fn unique_tenant(name: &str) -> String {
    let run = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("tenant-paging-{name}-{run}")
}

async fn connect(page_size: usize) -> Arc<PostgresStore> {
    let url = common::get_test_database_url().await;
    let store = PostgresStore::connect_for_tests(&url)
        .await
        .expect("connect");
    store.set_subscribe_page_size(page_size);
    store
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
        payload: vec![nonce as u8],
    }
}

/// Append `count` events to `aggregate_id` after `after_nonce`, one request.
/// Returns their global nonces.
async fn append(
    store: &PostgresStore,
    tenant: &str,
    aggregate_id: &str,
    after_nonce: u64,
    count: u64,
) -> Vec<u64> {
    store
        .append(proto::AppendRequest {
            tenant_id: tenant.into(),
            aggregate_id: aggregate_id.into(),
            aggregate_type: "Order".into(),
            expected_aggregate_nonce: after_nonce,
            idempotency_key: String::new(),
            events: (1..=count)
                .map(|i| event(tenant, aggregate_id, after_nonce + i))
                .collect(),
        })
        .await
        .expect("append");
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
    read.events
        .iter()
        .map(|e| e.meta.as_ref().expect("meta").global_nonce)
        .collect()
}

/// Append one event at a time to several aggregates, interleaved, so pages
/// mix aggregates. Returns `(aggregate_id, global_nonce)` in commit order.
async fn interleave(
    store: &PostgresStore,
    tenant: &str,
    aggregates: &[&str],
    per_aggregate: u64,
) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    for n in 1..=per_aggregate {
        for agg in aggregates {
            let g = append(store, tenant, agg, n - 1, 1).await;
            out.push((agg.to_string(), g[0]));
        }
    }
    out
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

/// Delivered `(aggregate_id, global_nonce)` until the first caught-up marker.
async fn until_caught_up(stream: &mut StoreStream<proto::SubscribeResponse>) -> Vec<(String, u64)> {
    let mut seen = Vec::new();
    loop {
        match next(stream).await {
            Some(Ok(resp)) => match resp.event {
                Some(ev) => {
                    let meta = ev.meta.expect("meta");
                    seen.push((meta.aggregate_id, meta.global_nonce));
                }
                None => return seen,
            },
            other => panic!("expected events then a caught-up marker, got {other:?}"),
        }
    }
}

fn nonces(events: &[(String, u64)]) -> Vec<u64> {
    events.iter().map(|(_, g)| *g).collect()
}

#[tokio::test]
async fn replay_delivers_a_multi_page_history_in_order_one_page_at_a_time() {
    const PAGE: usize = 7;
    let tenant = unique_tenant("multi-page");
    let writer = connect(PAGE).await;
    let expected = interleave(&writer, &tenant, &["Order-1", "Order-2", "Other-1"], 34).await;
    assert_eq!(expected.len(), 102);
    // Another tenant's events between ours must not count against a page.
    append(&writer, &unique_tenant("noise"), "Order-1", 0, 20).await;

    let store = connect(PAGE).await;
    let mut stream = store.subscribe(request(&tenant, "", 0));
    match next(&mut stream).await {
        Some(Ok(proto::SubscribeResponse { event: Some(ev) })) => {
            assert_eq!(ev.meta.expect("meta").global_nonce, expected[0].1)
        }
        other => panic!("expected the first event, got {other:?}"),
    }
    assert_eq!(
        store.subscribe_page_queries(),
        1,
        "the first event must come from the first page alone, not the full history"
    );

    let mut seen = vec![expected[0].clone()];
    seen.extend(until_caught_up(&mut stream).await);
    assert_eq!(seen, expected, "every event once, in global order");
    // 14 full pages (98 events), then a short page of 4 ends the replay.
    assert_eq!(store.subscribe_page_queries(), 15);
}

#[tokio::test]
async fn replay_ending_on_a_full_page_sends_one_caught_up_marker_then_goes_live() {
    const PAGE: usize = 5;
    let tenant = unique_tenant("exact-pages");
    let store = connect(PAGE).await;
    let expected = append(&store, &tenant, "Order-1", 0, 10).await;

    let mut stream = store.subscribe(request(&tenant, "", 0));
    assert_eq!(nonces(&until_caught_up(&mut stream).await), expected);
    // Two full pages, then an empty one proves the end.
    assert_eq!(store.subscribe_page_queries(), 3);

    // Live continues right after the last replayed event.
    let live = append(&store, &tenant, "Order-1", 10, 1).await;
    loop {
        match next(&mut stream).await {
            Some(Ok(proto::SubscribeResponse { event: None })) => continue, // keep-alive
            Some(Ok(proto::SubscribeResponse { event: Some(ev) })) => {
                assert_eq!(ev.meta.expect("meta").global_nonce, live[0]);
                break;
            }
            other => panic!("expected the live event, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn replay_from_the_middle_of_a_page_is_inclusive() {
    const PAGE: usize = 4;
    let tenant = unique_tenant("from-mid");
    let store = connect(PAGE).await;
    let all = append(&store, &tenant, "Order-1", 0, 11).await;

    let mut stream = store.subscribe(request(&tenant, "", all[5]));
    assert_eq!(
        nonces(&until_caught_up(&mut stream).await),
        all[5..].to_vec()
    );
}

#[tokio::test]
async fn prefix_filtered_replay_pages_only_matching_events() {
    const PAGE: usize = 3;
    let tenant = unique_tenant("prefix-pages");
    let store = connect(PAGE).await;
    let all = interleave(&store, &tenant, &["Order-1", "Other-1", "Order-2"], 8).await;
    let expected: Vec<_> = all
        .iter()
        .filter(|(agg, _)| agg.starts_with("Order-"))
        .cloned()
        .collect();

    let mut stream = store.subscribe(request(&tenant, "Order-", 0));
    assert_eq!(until_caught_up(&mut stream).await, expected);
}

#[tokio::test]
async fn live_burst_larger_than_a_page_is_delivered_in_order() {
    const PAGE: usize = 4;
    let tenant = unique_tenant("live-burst");
    let store = connect(PAGE).await;
    let mut stream = store.subscribe(request(&tenant, "", 0));
    assert!(until_caught_up(&mut stream).await.is_empty());

    let burst = append(&store, &tenant, "Order-1", 0, 23).await;
    let mut seen = Vec::new();
    while seen.len() < burst.len() {
        match next(&mut stream).await {
            Some(Ok(resp)) => {
                if let Some(ev) = resp.event {
                    seen.push(ev.meta.expect("meta").global_nonce);
                }
            }
            other => panic!("expected live events, got {other:?}"),
        }
    }
    assert_eq!(seen, burst);
}

/// Appends race a paging replay that started at 0. Each page reads a fresh
/// snapshot; commit-ordered nonces (#337) mean no snapshot can show a nonce
/// while a lower one of the tenant is still in flight, so nothing is skipped
/// at a page boundary or at the replay/live boundary.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn paging_replay_racing_concurrent_appends_loses_nothing() {
    const WRITERS: usize = 8;
    const PER_WRITER: u64 = 20;
    let tenant = unique_tenant("race");
    let store = connect(3).await;
    let mut committed = append(&store, &tenant, "Seed-1", 0, 10).await;

    let mut writers = Vec::new();
    for w in 0..WRITERS {
        let store = store.clone();
        let tenant = tenant.clone();
        writers.push(tokio::spawn(async move {
            let agg = format!("Race-{w}");
            let mut out = Vec::new();
            for n in 1..=PER_WRITER {
                out.extend(append(&store, &tenant, &agg, n - 1, 1).await);
            }
            out
        }));
    }

    let mut stream = store.subscribe(request(&tenant, "", 0));
    let total = 10 + WRITERS * PER_WRITER as usize;
    let mut seen = Vec::with_capacity(total);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    while seen.len() < total {
        let item = tokio::time::timeout_at(deadline, stream.next())
            .await
            .unwrap_or_else(|_| panic!("only {} of {total} events arrived", seen.len()));
        match item {
            Some(Ok(resp)) => {
                if let Some(ev) = resp.event {
                    seen.push(ev.meta.expect("meta").global_nonce);
                }
            }
            other => panic!("subscription failed: {other:?}"),
        }
    }
    for w in writers {
        committed.extend(w.await.expect("join"));
    }
    committed.sort_unstable();
    assert!(
        seen.windows(2).all(|w| w[0] < w[1]),
        "strictly increasing, no duplicates"
    );
    assert_eq!(seen, committed, "every committed event exactly once");
}

/// The stream must yield UNAVAILABLE naming `resume_from`, then end.
async fn expect_unavailable_then_end(
    stream: &mut StoreStream<proto::SubscribeResponse>,
    resume_from: u64,
) {
    match next(stream).await {
        Some(Err(e)) => {
            assert_eq!(e.to_status().code(), Code::Unavailable, "{e:?}");
            let resume = format!("resume from global_nonce {resume_from} ");
            assert!(e.to_string().contains(&resume), "{e}");
        }
        other => panic!("expected UNAVAILABLE, got {other:?}"),
    }
    assert!(
        next(stream).await.is_none(),
        "stream must end after the error"
    );
}

#[tokio::test]
async fn query_failure_mid_pagination_keeps_the_cursor_and_recovers() {
    const PAGE: usize = 5;
    let tenant = unique_tenant("fault-mid");
    let writer = connect(PAGE).await;
    let all = append(&writer, &tenant, "Order-1", 0, 20).await;

    let subscriber = connect(PAGE).await;
    let mut stream = subscriber.subscribe(request(&tenant, "", 0));
    // Seven events: the whole first page and two of the second.
    let mut delivered = Vec::new();
    for _ in 0..7 {
        match next(&mut stream).await {
            Some(Ok(proto::SubscribeResponse { event: Some(ev) })) => {
                delivered.push(ev.meta.expect("meta").global_nonce)
            }
            other => panic!("expected an event, got {other:?}"),
        }
    }
    assert_eq!(subscriber.subscribe_page_queries(), 2);

    subscriber.pool().close().await;
    // The rest of the page already fetched is still delivered, then the
    // next page query fails: no caught-up marker, resume after the last one.
    for _ in 0..3 {
        match next(&mut stream).await {
            Some(Ok(proto::SubscribeResponse { event: Some(ev) })) => {
                delivered.push(ev.meta.expect("meta").global_nonce)
            }
            other => panic!("expected a buffered event, got {other:?}"),
        }
    }
    assert_eq!(delivered, all[..10].to_vec());
    expect_unavailable_then_end(&mut stream, all[9] + 1).await;

    let recovered = connect(PAGE).await;
    let mut stream = recovered.subscribe(request(&tenant, "", all[9] + 1));
    assert_eq!(
        nonces(&until_caught_up(&mut stream).await),
        all[10..].to_vec()
    );
}

/// Insert one row directly; `valid = false` stores headers the decoder
/// rejects (a JSON number where a string is required).
async fn insert_row(store: &PostgresStore, tenant: &str, nonce: i64, valid: bool) -> u64 {
    let headers = if valid {
        r#"{"k": "v"}"#
    } else {
        r#"{"k": 1}"#
    };
    let global: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO events (
            tenant_id, aggregate_id, aggregate_type, aggregate_nonce,
            event_id, event_type, event_version, content_type,
            recorded_time_unix_ms, headers, payload
        ) VALUES ($1, 'Order-1', 'Order', $2, $3, 'Happened', 1,
                  'application/octet-stream', 0, $4::jsonb, $5)
        RETURNING global_nonce
        "#,
    )
    .bind(tenant)
    .bind(nonce)
    .bind(format!("{tenant}-{nonce}"))
    .bind(headers)
    .bind(vec![1u8])
    .fetch_one(store.pool())
    .await
    .expect("insert row");
    global as u64
}

/// Rows 1..=total, row `bad_at` undecodable. Replays with page size `page`
/// and checks it stops right before the bad row with DATA_LOSS.
async fn replay_stops_at_bad_row(name: &str, page: usize, total: i64, bad_at: i64) {
    let tenant = unique_tenant(name);
    let store = connect(page).await;
    let mut good = Vec::new();
    let mut bad = 0;
    for n in 1..=total {
        let g = insert_row(&store, &tenant, n, n != bad_at).await;
        if n == bad_at {
            bad = g;
        } else if n < bad_at {
            good.push(g);
        }
    }

    let mut stream = store.subscribe(request(&tenant, "", 0));
    let mut seen = Vec::new();
    let err = loop {
        match next(&mut stream).await {
            Some(Ok(proto::SubscribeResponse { event: Some(ev) })) => {
                seen.push(ev.meta.expect("meta").global_nonce)
            }
            Some(Ok(_)) => panic!("no caught-up marker past an undecodable row"),
            Some(Err(e)) => break e,
            None => panic!("stream ended without an error"),
        }
    };
    assert_eq!(seen, good, "exactly the events before the bad row");
    assert_eq!(err.to_status().code(), Code::DataLoss, "{err:?}");
    match err {
        StoreError::UndecodableEvent { global_nonce, .. } => assert_eq!(global_nonce, bad),
        other => panic!("expected UndecodableEvent, got {other:?}"),
    }
    assert!(next(&mut stream).await.is_none(), "stream must end");
}

#[tokio::test]
async fn undecodable_row_inside_a_later_page_stops_replay() {
    // Page 4: the bad row is the second of the second page.
    replay_stops_at_bad_row("bad-mid-page", 4, 9, 6).await;
}

#[tokio::test]
async fn undecodable_row_first_in_a_page_stops_replay() {
    // Page 4: the bad row opens the second page.
    replay_stops_at_bad_row("bad-page-start", 4, 9, 5).await;
}
