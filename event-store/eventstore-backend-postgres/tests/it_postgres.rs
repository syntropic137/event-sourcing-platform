mod common;

use eventstore_backend_postgres::PostgresStore;
use eventstore_core::proto;
use eventstore_core::EventStore;
use futures::StreamExt;
use sqlx::{query, query_scalar};
use tonic::Code;

// Use unique tenant IDs per test to ensure isolation when using shared testcontainer
const TENANT_END_TO_END: &str = "tenant-end-to-end";
const TENANT_IMMUTABILITY: &str = "tenant-immutability";
const TENANT_SEQUENCING: &str = "tenant-sequencing";

const AGGREGATE_ID: &str = "Order-1";
const AGGREGATE_TYPE: &str = "Order";

fn new_event(tenant_id: &str, nonce: u64, event_id: &str, event_type: &str) -> proto::EventData {
    proto::EventData {
        meta: Some(proto::EventMetadata {
            event_id: event_id.into(),
            aggregate_id: AGGREGATE_ID.into(),
            aggregate_type: AGGREGATE_TYPE.into(),
            aggregate_nonce: nonce,
            event_type: event_type.into(),
            event_version: 1,
            content_type: "application/octet-stream".into(),
            tenant_id: tenant_id.into(),
            ..Default::default()
        }),
        payload: format!("payload-{nonce}").into_bytes(),
    }
}

#[tokio::test]
async fn postgres_end_to_end_append_read_and_migrations() {
    let url = common::get_test_database_url().await;
    let store = PostgresStore::connect_for_tests(&url)
        .await
        .expect("connect+init");

    // migrations should have created tables
    let count: i64 = query_scalar("SELECT COUNT(*) FROM events WHERE tenant_id = $1")
        .bind(TENANT_END_TO_END)
        .fetch_one(store.pool())
        .await
        .expect("count events");
    assert_eq!(count, 0, "Test should start with clean tenant data");

    let append_res = store
        .append(proto::AppendRequest {
            tenant_id: TENANT_END_TO_END.into(),
            aggregate_id: AGGREGATE_ID.into(),
            aggregate_type: AGGREGATE_TYPE.into(),
            expected_aggregate_nonce: 0,
            idempotency_key: "batch-1".into(),
            events: vec![
                new_event(
                    TENANT_END_TO_END,
                    1,
                    "00000000-0000-0000-0000-000000000001",
                    "OrderSubmitted",
                ),
                new_event(
                    TENANT_END_TO_END,
                    2,
                    "00000000-0000-0000-0000-000000000002",
                    "OrderConfirmed",
                ),
            ],
        })
        .await
        .expect("append ok");
    assert_eq!(append_res.last_aggregate_nonce, 2);
    // Note: global_nonce is shared across all tenants, so we just check it's positive
    assert!(append_res.last_global_nonce > 0);

    // Read forward
    let rs = store
        .read_stream(proto::ReadStreamRequest {
            tenant_id: TENANT_END_TO_END.into(),
            aggregate_id: AGGREGATE_ID.into(),
            from_aggregate_nonce: 1,
            max_count: 10,
            forward: true,
        })
        .await
        .expect("read ok");
    assert_eq!(rs.events.len(), 2);
    let first_meta = rs.events[0].meta.as_ref().expect("meta");
    assert_eq!(first_meta.aggregate_nonce, 1);
    assert_eq!(first_meta.tenant_id, TENANT_END_TO_END);
    assert!(first_meta.global_nonce > 0);

    // Repeating append with identical idempotency key should short-circuit
    let replay_err = store
        .append(proto::AppendRequest {
            tenant_id: TENANT_END_TO_END.into(),
            aggregate_id: AGGREGATE_ID.into(),
            aggregate_type: AGGREGATE_TYPE.into(),
            expected_aggregate_nonce: 2,
            idempotency_key: "batch-1".into(),
            events: vec![new_event(
                TENANT_END_TO_END,
                3,
                "00000000-0000-0000-0000-000000000003",
                "OrderShipped",
            )],
        })
        .await
        .expect_err("idempotent replay with different payload should error");
    assert!(matches!(
        replay_err,
        eventstore_core::StoreError::AlreadyExists(_)
    ));

    // Concurrency error: wrong expected version
    let err = store
        .append(proto::AppendRequest {
            tenant_id: TENANT_END_TO_END.into(),
            aggregate_id: AGGREGATE_ID.into(),
            aggregate_type: AGGREGATE_TYPE.into(),
            expected_aggregate_nonce: 1,
            idempotency_key: "batch-2".into(),
            events: vec![new_event(
                TENANT_END_TO_END,
                3,
                "00000000-0000-0000-0000-000000000004",
                "OrderShipped",
            )],
        })
        .await
        .expect_err("should fail concurrency");
    let status = err.to_status();
    assert_eq!(status.code(), Code::Aborted);
}

#[tokio::test]
async fn postgres_immutability_triggers_block_update_delete() {
    let url = common::get_test_database_url().await;
    let store = PostgresStore::connect_for_tests(&url)
        .await
        .expect("connect");

    store
        .append(proto::AppendRequest {
            tenant_id: TENANT_IMMUTABILITY.into(),
            aggregate_id: "Immut-1".into(),
            aggregate_type: "Immut".into(),
            expected_aggregate_nonce: 0,
            idempotency_key: String::new(),
            events: vec![proto::EventData {
                meta: Some(proto::EventMetadata {
                    event_id: "11111111-1111-1111-1111-111111111111".into(),
                    aggregate_id: "Immut-1".into(),
                    aggregate_type: "Immut".into(),
                    aggregate_nonce: 1,
                    event_type: "Created".into(),
                    event_version: 1,
                    content_type: "application/octet-stream".into(),
                    tenant_id: TENANT_IMMUTABILITY.into(),
                    ..Default::default()
                }),
                payload: b"x".to_vec(),
            }],
        })
        .await
        .expect("append ok");

    let upd = query("UPDATE events SET event_type = 'Hacked' WHERE tenant_id = $1")
        .bind(TENANT_IMMUTABILITY)
        .execute(store.pool())
        .await;
    assert!(upd.is_err());

    let del = query("DELETE FROM events WHERE tenant_id = $1")
        .bind(TENANT_IMMUTABILITY)
        .execute(store.pool())
        .await;
    assert!(del.is_err());
}

#[tokio::test]
async fn postgres_sequencing_trigger_enforces_prev_plus_one() {
    let url = common::get_test_database_url().await;
    let store = PostgresStore::connect_for_tests(&url)
        .await
        .expect("connect");

    store
        .append(proto::AppendRequest {
            tenant_id: TENANT_SEQUENCING.into(),
            aggregate_id: "Seq-1".into(),
            aggregate_type: "Seq".into(),
            expected_aggregate_nonce: 0,
            idempotency_key: String::new(),
            events: vec![proto::EventData {
                meta: Some(proto::EventMetadata {
                    event_id: "22222222-2222-2222-2222-222222222222".into(),
                    aggregate_id: "Seq-1".into(),
                    aggregate_type: "Seq".into(),
                    aggregate_nonce: 1,
                    event_type: "Created".into(),
                    event_version: 1,
                    content_type: "application/octet-stream".into(),
                    tenant_id: TENANT_SEQUENCING.into(),
                    ..Default::default()
                }),
                payload: b"1".to_vec(),
            }],
        })
        .await
        .expect("append ok");

    // Force an out-of-order insert via raw SQL (skipping nonce)
    let ins = query(
        r#"INSERT INTO events (
            tenant_id, aggregate_id, aggregate_type, aggregate_nonce,
            event_id, event_type, event_version, content_type,
            content_schema, correlation_id, causation_id, actor_id,
            timestamp_unix_ms, recorded_time_unix_ms, payload_sha256, headers, payload
        ) VALUES (
            $1, $2, $3, $4,
            $5, $6, $7, $8,
            NULL, NULL, NULL, NULL,
            0, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000, NULL, '{}'::jsonb, $9
        )"#,
    )
    .bind(TENANT_SEQUENCING)
    .bind("Seq-1")
    .bind("Seq")
    .bind(3_i64)
    .bind("33333333-3333-3333-3333-333333333333")
    .bind("Skipped")
    .bind(1_i32)
    .bind("application/octet-stream")
    .bind(b"oops".to_vec())
    .execute(store.pool())
    .await;
    assert!(ins.is_err());

    let res2 = store
        .append(proto::AppendRequest {
            tenant_id: TENANT_SEQUENCING.into(),
            aggregate_id: "Seq-1".into(),
            aggregate_type: "Seq".into(),
            expected_aggregate_nonce: 1,
            idempotency_key: String::new(),
            events: vec![proto::EventData {
                meta: Some(proto::EventMetadata {
                    event_id: "22222222-2222-2222-2222-222222222223".into(),
                    aggregate_id: "Seq-1".into(),
                    aggregate_type: "Seq".into(),
                    aggregate_nonce: 2,
                    event_type: "Confirmed".into(),
                    event_version: 1,
                    content_type: "application/octet-stream".into(),
                    tenant_id: TENANT_SEQUENCING.into(),
                    ..Default::default()
                }),
                payload: b"2".to_vec(),
            }],
        })
        .await
        .expect("append nonce 2");
    assert_eq!(res2.last_aggregate_nonce, 2);
}

// ============================================================================
// Subscribe Tests - Regression tests for cursor advancement bug (ADR-013)
// ============================================================================

const TENANT_SUBSCRIBE: &str = "tenant-subscribe";

/// Helper to create events with unique IDs for subscribe tests
fn new_subscribe_event(
    tenant_id: &str,
    aggregate_id: &str,
    nonce: u64,
    event_type: &str,
) -> proto::EventData {
    proto::EventData {
        meta: Some(proto::EventMetadata {
            event_id: format!("sub-evt-{aggregate_id}-{nonce}"),
            aggregate_id: aggregate_id.into(),
            aggregate_type: "SubscribeTest".into(),
            aggregate_nonce: nonce,
            event_type: event_type.into(),
            event_version: 1,
            content_type: "application/octet-stream".into(),
            tenant_id: tenant_id.into(),
            ..Default::default()
        }),
        payload: format!("payload-{nonce}").into_bytes(),
    }
}

/// Regression test for ADR-013: Subscribe cursor advancement bug
///
/// This test verifies that ALL events are yielded by the subscribe stream,
/// and that events are delivered in global_nonce order.
///
/// The bug: Cursor was advanced to max(all_read_positions) BEFORE events
/// were yielded, meaning if connection dropped mid-batch, events could be lost.
#[tokio::test]
async fn postgres_subscribe_yields_all_events_in_order() {
    let url = common::get_test_database_url().await;
    let store = PostgresStore::connect_for_tests(&url)
        .await
        .expect("connect");

    // Append multiple events across different aggregates to test global ordering
    for i in 1..=3 {
        store
            .append(proto::AppendRequest {
                tenant_id: TENANT_SUBSCRIBE.into(),
                aggregate_id: format!("Sub-{i}"),
                aggregate_type: "SubscribeTest".into(),
                expected_aggregate_nonce: 0,
                idempotency_key: String::new(),
                events: vec![new_subscribe_event(
                    TENANT_SUBSCRIBE,
                    &format!("Sub-{i}"),
                    1,
                    "Created",
                )],
            })
            .await
            .expect("append");
    }

    // Subscribe from the beginning
    let mut stream = store.subscribe(proto::SubscribeRequest {
        tenant_id: TENANT_SUBSCRIBE.into(),
        aggregate_id_prefix: "Sub-".into(),
        from_global_nonce: 0,
    });

    // Collect all events (with timeout to prevent infinite loop)
    let mut received_events = Vec::new();
    let mut empty_count = 0;
    const MAX_EMPTY: usize = 3; // After 3 empty responses, we've caught up

    while let Some(result) = tokio::time::timeout(std::time::Duration::from_secs(5), stream.next())
        .await
        .ok()
        .flatten()
    {
        match result {
            Ok(response) => {
                if let Some(event) = response.event {
                    received_events.push(event);
                    empty_count = 0;
                } else {
                    empty_count += 1;
                    if empty_count >= MAX_EMPTY {
                        break; // Caught up to live
                    }
                }
            }
            Err(e) => panic!("Subscribe error: {e:?}"),
        }
    }

    // Verify we received ALL 3 events
    assert_eq!(
        received_events.len(),
        3,
        "Expected 3 events, got {}. Events: {:?}",
        received_events.len(),
        received_events
            .iter()
            .map(|e| e.meta.as_ref().map(|m| m.global_nonce))
            .collect::<Vec<_>>()
    );

    // Verify global_nonce ordering is strictly increasing
    let nonces: Vec<u64> = received_events
        .iter()
        .filter_map(|e| e.meta.as_ref().map(|m| m.global_nonce))
        .collect();

    for i in 1..nonces.len() {
        assert!(nonces[i] > nonces[i - 1], "Events not in order: {nonces:?}");
    }
}

/// Test that subscribe correctly handles batches of events in Live phase
///
/// This specifically tests the bug where Live phase would advance cursor to
/// max position of all read rows, but only yield the first event.
#[tokio::test]
async fn postgres_subscribe_live_phase_yields_all_batch_events() {
    let url = common::get_test_database_url().await;
    let store = PostgresStore::connect_for_tests(&url)
        .await
        .expect("connect");

    // Use a unique aggregate ID to avoid conflicts with other tests
    let aggregate_id = format!(
        "LiveBatch-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );

    // First, create an initial event
    store
        .append(proto::AppendRequest {
            tenant_id: TENANT_SUBSCRIBE.into(),
            aggregate_id: aggregate_id.clone(),
            aggregate_type: "SubscribeTest".into(),
            expected_aggregate_nonce: 0,
            idempotency_key: String::new(),
            events: vec![new_subscribe_event(
                TENANT_SUBSCRIBE,
                &aggregate_id,
                1,
                "Initial",
            )],
        })
        .await
        .expect("initial append");

    // Get the global_nonce of the initial event
    let read_result = store
        .read_stream(proto::ReadStreamRequest {
            tenant_id: TENANT_SUBSCRIBE.into(),
            aggregate_id: aggregate_id.clone(),
            from_aggregate_nonce: 1,
            max_count: 1,
            forward: true,
        })
        .await
        .expect("read");

    let initial_global_nonce = read_result.events[0].meta.as_ref().unwrap().global_nonce;

    // Start subscribing from AFTER the initial event (so we're in "live" mode)
    let mut stream = store.subscribe(proto::SubscribeRequest {
        tenant_id: TENANT_SUBSCRIBE.into(),
        aggregate_id_prefix: aggregate_id.clone(),
        from_global_nonce: initial_global_nonce + 1,
    });

    // Skip past the initial empty/catch-up responses
    let mut caught_up = false;
    while !caught_up {
        if let Some(Ok(response)) =
            tokio::time::timeout(std::time::Duration::from_millis(500), stream.next())
                .await
                .ok()
                .flatten()
        {
            if response.event.is_none() {
                caught_up = true;
            }
        } else {
            caught_up = true;
        }
    }

    // Now append a BATCH of events (multiple events in one append)
    store
        .append(proto::AppendRequest {
            tenant_id: TENANT_SUBSCRIBE.into(),
            aggregate_id: aggregate_id.clone(),
            aggregate_type: "SubscribeTest".into(),
            expected_aggregate_nonce: 1,
            idempotency_key: String::new(),
            events: vec![
                new_subscribe_event(TENANT_SUBSCRIBE, &aggregate_id, 2, "BatchEvent1"),
                new_subscribe_event(TENANT_SUBSCRIBE, &aggregate_id, 3, "BatchEvent2"),
                new_subscribe_event(TENANT_SUBSCRIBE, &aggregate_id, 4, "BatchEvent3"),
            ],
        })
        .await
        .expect("batch append");

    // Collect events from the live stream
    let mut live_events = Vec::new();
    let mut empty_count = 0;

    while live_events.len() < 3 && empty_count < 5 {
        if let Some(result) = tokio::time::timeout(std::time::Duration::from_secs(2), stream.next())
            .await
            .ok()
            .flatten()
        {
            match result {
                Ok(response) => {
                    if let Some(event) = response.event {
                        live_events.push(event);
                    } else {
                        empty_count += 1;
                    }
                }
                Err(e) => panic!("Subscribe error: {e:?}"),
            }
        } else {
            break;
        }
    }

    // Verify we received ALL 3 batch events
    assert_eq!(
        live_events.len(),
        3,
        "Expected 3 live events, got {}. Event types: {:?}",
        live_events.len(),
        live_events
            .iter()
            .map(|e| e.meta.as_ref().map(|m| m.event_type.clone()))
            .collect::<Vec<_>>()
    );

    // Verify they arrived in correct order
    let event_types: Vec<&str> = live_events
        .iter()
        .filter_map(|e| e.meta.as_ref().map(|m| m.event_type.as_str()))
        .collect();

    assert_eq!(
        event_types,
        vec!["BatchEvent1", "BatchEvent2", "BatchEvent3"],
        "Events not in correct order"
    );
}

/// Regression test for off-by-one bug in subscribe when starting before event exists.
///
/// This tests the race condition where:
/// 1. Subscription starts at position N (where no event exists yet)
/// 2. Initial replay query returns empty
/// 3. Subscription transitions to Live phase with cursor = N
/// 4. Event is created at position N
/// 5. Live polling with `> N` would MISS the event at N (BUG!)
///
/// The fix: When replay is empty, subtract 1 from cursor before transitioning to Live,
/// so that Live polling with `> (N-1)` catches events at N.
#[tokio::test]
async fn postgres_subscribe_catches_event_at_from_position_when_created_after_subscribe_starts() {
    let url = common::get_test_database_url().await;
    let store = PostgresStore::connect_for_tests(&url)
        .await
        .expect("connect");

    // Use a unique aggregate ID to avoid conflicts
    let aggregate_id = format!(
        "OffByOne-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );

    // First, create an initial event to establish a known global_nonce
    store
        .append(proto::AppendRequest {
            tenant_id: TENANT_SUBSCRIBE.into(),
            aggregate_id: aggregate_id.clone(),
            aggregate_type: "SubscribeTest".into(),
            expected_aggregate_nonce: 0,
            idempotency_key: String::new(),
            events: vec![new_subscribe_event(
                TENANT_SUBSCRIBE,
                &aggregate_id,
                1,
                "Setup",
            )],
        })
        .await
        .expect("setup append");

    // Get the global_nonce of the setup event
    let read_result = store
        .read_stream(proto::ReadStreamRequest {
            tenant_id: TENANT_SUBSCRIBE.into(),
            aggregate_id: aggregate_id.clone(),
            from_aggregate_nonce: 1,
            max_count: 1,
            forward: true,
        })
        .await
        .expect("read");

    let setup_global_nonce = read_result.events[0].meta.as_ref().unwrap().global_nonce;

    // Start subscription from AFTER the setup event (position that doesn't exist yet)
    // This simulates the race condition where we subscribe before an event is created
    let target_position = setup_global_nonce + 1;

    let mut stream = store.subscribe(proto::SubscribeRequest {
        tenant_id: TENANT_SUBSCRIBE.into(),
        aggregate_id_prefix: aggregate_id.clone(),
        from_global_nonce: target_position,
    });

    // Wait for the subscription to enter Live phase (indicated by empty response)
    let mut entered_live = false;
    for _ in 0..5 {
        if let Some(Ok(response)) =
            tokio::time::timeout(std::time::Duration::from_millis(300), stream.next())
                .await
                .ok()
                .flatten()
        {
            if response.event.is_none() {
                entered_live = true;
                break;
            }
        } else {
            entered_live = true;
            break;
        }
    }
    assert!(entered_live, "Subscription should enter live phase");

    // NOW create the event at the position we subscribed from
    // This is the critical test: will the subscription catch this event?
    store
        .append(proto::AppendRequest {
            tenant_id: TENANT_SUBSCRIBE.into(),
            aggregate_id: aggregate_id.clone(),
            aggregate_type: "SubscribeTest".into(),
            expected_aggregate_nonce: 1,
            idempotency_key: String::new(),
            events: vec![new_subscribe_event(
                TENANT_SUBSCRIBE,
                &aggregate_id,
                2,
                "TargetEvent",
            )],
        })
        .await
        .expect("target append");

    // Collect events from the live stream
    let mut received_events = Vec::new();
    let mut poll_attempts = 0;
    const MAX_POLLS: usize = 10;

    while received_events.is_empty() && poll_attempts < MAX_POLLS {
        poll_attempts += 1;
        if let Some(result) =
            tokio::time::timeout(std::time::Duration::from_millis(500), stream.next())
                .await
                .ok()
                .flatten()
        {
            match result {
                Ok(response) => {
                    if let Some(event) = response.event {
                        received_events.push(event);
                    }
                }
                Err(e) => panic!("Subscribe error: {e:?}"),
            }
        }
    }

    // CRITICAL ASSERTION: We MUST receive the event that was created after subscription started
    assert_eq!(
        received_events.len(),
        1,
        "Regression: Subscription missed event at from_global_nonce position! \
         This is the off-by-one bug where Live phase uses `> cursor` instead of `>= cursor` \
         when no events existed during initial replay. Polls: {poll_attempts}"
    );

    // Verify it's the correct event
    let event_type = received_events[0]
        .meta
        .as_ref()
        .map(|m| m.event_type.as_str())
        .unwrap_or("");
    assert_eq!(
        event_type, "TargetEvent",
        "Received wrong event type: {event_type}"
    );

    // Verify global_nonce is at or after target position
    let event_nonce = received_events[0]
        .meta
        .as_ref()
        .map(|m| m.global_nonce)
        .unwrap_or(0);
    assert!(
        event_nonce >= target_position,
        "Event nonce {event_nonce} should be >= target {target_position}"
    );
}
