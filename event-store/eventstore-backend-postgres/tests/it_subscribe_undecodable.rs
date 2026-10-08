//! A subscription must stop at an undecodable stored event, never skip it
//! (#351).
//!
//! Before the fix, a row that `row_to_event()` could not decode was logged
//! and skipped: later valid events advanced the cursor past it, and an
//! all-invalid batch advanced the cursor explicitly. Consumers then built
//! incomplete projections without any error.
//!
//! These tests store a decoder-invalid row (headers that are not a
//! string-to-string map) and check that the subscription delivers every
//! event before it, then yields a typed `DATA_LOSS` error naming its
//! position, never delivers a later position, and ends. Reconnecting from
//! the consumer's checkpoint hits the same error again: nothing can be
//! checkpointed past the bad row by accident.

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
    format!("tenant-undecodable-{name}-{run}")
}

async fn connect() -> Arc<PostgresStore> {
    let url = common::get_test_database_url().await;
    PostgresStore::connect_for_tests(&url)
        .await
        .expect("connect")
}

/// Insert one event row directly. `valid_headers = false` stores headers the
/// decoder rejects (a JSON number where a string is required).
async fn insert_row(
    store: &PostgresStore,
    tenant: &str,
    aggregate_id: &str,
    nonce: i64,
    valid_headers: bool,
) -> u64 {
    let headers = if valid_headers {
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
        ) VALUES ($1, $2, 'Order', $3, $4, 'Happened', 1,
                  'application/octet-stream', 0, $5::jsonb, $6)
        RETURNING global_nonce
        "#,
    )
    .bind(tenant)
    .bind(aggregate_id)
    .bind(nonce)
    .bind(format!("{tenant}-{aggregate_id}-{nonce}"))
    .bind(headers)
    .bind(b"secret-payload".to_vec())
    .fetch_one(store.pool())
    .await
    .expect("insert row");
    global as u64
}

fn request(tenant: &str, from: u64) -> proto::SubscribeRequest {
    proto::SubscribeRequest {
        tenant_id: tenant.into(),
        aggregate_id_prefix: String::new(),
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

/// Drain the stream: delivered nonces (keep-alives skipped) and the
/// terminating error. Panics if the stream ends without an error.
async fn drain_until_error(
    stream: &mut StoreStream<proto::SubscribeResponse>,
    stop_at_caught_up: bool,
) -> (Vec<u64>, Option<StoreError>) {
    let mut seen = Vec::new();
    let deadline = tokio::time::Instant::now() + 2 * STEP;
    loop {
        assert!(
            tokio::time::Instant::now() < deadline,
            "no error surfaced (only keep-alives); delivered {seen:?}"
        );
        match next(stream).await {
            Some(Ok(resp)) => match resp.event {
                Some(ev) => seen.push(ev.meta.expect("meta").global_nonce),
                None if stop_at_caught_up => return (seen, None),
                None => {}
            },
            Some(Err(e)) => {
                assert!(next(stream).await.is_none(), "stream must end after error");
                return (seen, Some(e));
            }
            None => panic!("stream ended without an error; delivered {seen:?}"),
        }
    }
}

fn assert_data_integrity_error(e: &StoreError, bad: u64) {
    assert_eq!(e.to_status().code(), Code::DataLoss, "{e:?}");
    match e {
        StoreError::UndecodableEvent { global_nonce, .. } => assert_eq!(*global_nonce, bad),
        other => panic!("expected UndecodableEvent, got {other:?}"),
    }
    let msg = e.to_string();
    assert!(msg.contains(&format!("global_nonce {bad}")), "{msg}");
    assert!(!msg.contains("secret-payload"), "payload leaked: {msg}");
    assert!(msg.contains("column 'headers'"), "names the column: {msg}");
    assert!(
        !msg.contains("invalid type"),
        "decoder text can quote stored values: {msg}"
    );
}

#[tokio::test]
async fn replay_stops_at_undecodable_row_between_valid_rows() {
    let tenant = unique_tenant("replay");
    let store = connect().await;
    // This is the behavior the backend advertises as a capability.
    assert!(store
        .capabilities()
        .contains(&eventstore_core::capabilities::UNDECODABLE_EVENTS_SURFACED));
    let good1 = insert_row(&store, &tenant, "Order-1", 1, true).await;
    let bad = insert_row(&store, &tenant, "Order-1", 2, false).await;
    let _good2 = insert_row(&store, &tenant, "Order-1", 3, true).await;

    let mut stream = store.subscribe(request(&tenant, 0));
    let (seen, err) = drain_until_error(&mut stream, true).await;
    assert_eq!(seen, vec![good1], "only events before the bad row");
    assert_data_integrity_error(&err.expect("no caught-up past a bad row"), bad);

    // Reconnect from the checkpoint: the same error, never a later event.
    let mut stream = store.subscribe(request(&tenant, good1 + 1));
    let (seen, err) = drain_until_error(&mut stream, true).await;
    assert!(seen.is_empty(), "{seen:?}");
    assert_data_integrity_error(&err.expect("error again"), bad);
}

#[tokio::test]
async fn live_stops_at_undecodable_row_between_valid_rows() {
    let tenant = unique_tenant("live");
    let store = connect().await;

    let mut stream = store.subscribe(request(&tenant, 0));
    let (seen, err) = drain_until_error(&mut stream, true).await;
    assert!(
        seen.is_empty() && err.is_none(),
        "empty tenant reaches live"
    );

    // One live batch: valid, invalid, valid. The trailing append notifies.
    let good1 = insert_row(&store, &tenant, "Order-1", 1, true).await;
    let bad = insert_row(&store, &tenant, "Order-1", 2, false).await;
    let good2 = store
        .append(proto::AppendRequest {
            tenant_id: tenant.clone(),
            aggregate_id: "Order-2".into(),
            aggregate_type: "Order".into(),
            expected_aggregate_nonce: 0,
            idempotency_key: String::new(),
            events: vec![proto::EventData {
                meta: Some(proto::EventMetadata {
                    event_id: format!("{tenant}-good2"),
                    aggregate_nonce: 1,
                    event_type: "Happened".into(),
                    event_version: 1,
                    ..Default::default()
                }),
                payload: vec![1],
            }],
        })
        .await
        .expect("append")
        .last_global_nonce;
    assert!(good2 > bad);

    let (seen, err) = drain_until_error(&mut stream, false).await;
    assert_eq!(seen, vec![good1], "only events before the bad row");
    assert_data_integrity_error(&err.expect("error"), bad);
}

#[tokio::test]
async fn all_invalid_batch_cannot_be_checkpointed_past() {
    let tenant = unique_tenant("all-invalid");
    let store = connect().await;
    let bad1 = insert_row(&store, &tenant, "Order-1", 1, false).await;
    let _bad2 = insert_row(&store, &tenant, "Order-1", 2, false).await;

    // Replay: the whole batch is invalid. No event, no caught-up marker.
    let mut stream = store.subscribe(request(&tenant, 0));
    let (seen, err) = drain_until_error(&mut stream, true).await;
    assert!(seen.is_empty(), "{seen:?}");
    assert_data_integrity_error(&err.expect("no silent catch-up"), bad1);

    // The consumer never received a position to checkpoint; reconnecting
    // from its start position fails at the same row every time.
    let mut stream = store.subscribe(request(&tenant, 0));
    let (_, err) = drain_until_error(&mut stream, true).await;
    assert_data_integrity_error(&err.expect("still failing"), bad1);

    // Live: an all-invalid batch arriving after catch-up.
    let live_tenant = unique_tenant("all-invalid-live");
    let mut stream = store.subscribe(request(&live_tenant, 0));
    let (seen, err) = drain_until_error(&mut stream, true).await;
    assert!(seen.is_empty() && err.is_none());
    let live_bad = insert_row(&store, &live_tenant, "Order-1", 1, false).await;
    let _ = insert_row(&store, &live_tenant, "Order-1", 2, false).await;
    let (seen, err) = drain_until_error(&mut stream, false).await;
    assert!(seen.is_empty(), "{seen:?}");
    assert_data_integrity_error(&err.expect("live error"), live_bad);
}

#[tokio::test]
async fn reads_report_undecodable_row_instead_of_panicking() {
    let tenant = unique_tenant("read-all");
    let store = connect().await;
    let bad = insert_row(&store, &tenant, "Order-1", 1, false).await;
    let err = store
        .read_all(proto::ReadAllRequest {
            tenant_id: tenant.clone(),
            from_global_nonce: 0,
            max_count: 10,
            forward: true,
        })
        .await
        .expect_err("undecodable row must fail the read");
    assert_data_integrity_error(&err, bad);

    let err = store
        .read_stream(proto::ReadStreamRequest {
            tenant_id: tenant,
            aggregate_id: "Order-1".into(),
            from_aggregate_nonce: 1,
            max_count: 10,
            forward: true,
        })
        .await
        .expect_err("undecodable row must fail read_stream");
    assert_data_integrity_error(&err, bad);
}

/// Pages fetch one row past `max_count` to set `is_end` (#403). That
/// lookahead row is not decoded: a page that stops just before an
/// undecodable event succeeds, and the next page reports it.
#[tokio::test]
async fn page_ending_before_undecodable_row_succeeds() {
    let tenant = unique_tenant("lookahead");
    let store = connect().await;
    let good = insert_row(&store, &tenant, "Order-1", 1, true).await;
    let bad = insert_row(&store, &tenant, "Order-1", 2, false).await;

    let all_page = |from| proto::ReadAllRequest {
        tenant_id: tenant.clone(),
        from_global_nonce: from,
        max_count: 1,
        forward: true,
    };
    let page = store
        .read_all(all_page(0))
        .await
        .expect("page before the bad row");
    assert_eq!(page.events.len(), 1);
    assert!(!page.is_end);
    assert_eq!(page.next_from_global_nonce, good + 1);
    let err = store
        .read_all(all_page(page.next_from_global_nonce))
        .await
        .expect_err("next page holds the bad row");
    assert_data_integrity_error(&err, bad);

    let stream_page = |from| proto::ReadStreamRequest {
        tenant_id: tenant.clone(),
        aggregate_id: "Order-1".into(),
        from_aggregate_nonce: from,
        max_count: 1,
        forward: true,
    };
    let page = store
        .read_stream(stream_page(1))
        .await
        .expect("stream page before the bad row");
    assert_eq!(page.events.len(), 1);
    assert!(!page.is_end);
    let err = store
        .read_stream(stream_page(page.next_from_aggregate_nonce))
        .await
        .expect_err("next stream page holds the bad row");
    assert_data_integrity_error(&err, bad);
}

/// Backward, the lookahead row is below the page: a page above an
/// undecodable event succeeds, and the next page reports it.
#[tokio::test]
async fn backward_page_ending_above_undecodable_row_succeeds() {
    let tenant = unique_tenant("lookahead-back");
    let store = connect().await;
    let bad = insert_row(&store, &tenant, "Order-1", 1, false).await;
    let good = insert_row(&store, &tenant, "Order-1", 2, true).await;

    let all_page = |from| proto::ReadAllRequest {
        tenant_id: tenant.clone(),
        from_global_nonce: from,
        max_count: 1,
        forward: false,
    };
    let page = store
        .read_all(all_page(u64::MAX))
        .await
        .expect("page above the bad row");
    assert_eq!(page.events.len(), 1);
    assert!(!page.is_end);
    assert_eq!(page.next_from_global_nonce, good - 1);
    let err = store
        .read_all(all_page(page.next_from_global_nonce))
        .await
        .expect_err("next page holds the bad row");
    assert_data_integrity_error(&err, bad);

    let stream_page = |from| proto::ReadStreamRequest {
        tenant_id: tenant.clone(),
        aggregate_id: "Order-1".into(),
        from_aggregate_nonce: from,
        max_count: 1,
        forward: false,
    };
    let page = store
        .read_stream(stream_page(u64::MAX))
        .await
        .expect("stream page above the bad row");
    assert_eq!(page.events.len(), 1);
    assert!(!page.is_end);
    assert_eq!(page.next_from_aggregate_nonce, 1);
    let err = store
        .read_stream(stream_page(1))
        .await
        .expect_err("next stream page holds the bad row");
    assert_data_integrity_error(&err, bad);
}
