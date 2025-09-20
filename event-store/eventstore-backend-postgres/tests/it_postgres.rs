mod common;

use eventstore_backend_postgres::PostgresStore;
use eventstore_core::proto;
use eventstore_core::EventStore;
use sqlx::{query, query_scalar};
use tonic::Code;

const TENANT: &str = "tenant-a";
const AGGREGATE_ID: &str = "Order-1";
const AGGREGATE_TYPE: &str = "Order";

fn new_event(nonce: u64, event_id: &str, event_type: &str) -> proto::EventData {
    proto::EventData {
        meta: Some(proto::EventMetadata {
            event_id: event_id.into(),
            aggregate_id: AGGREGATE_ID.into(),
            aggregate_type: AGGREGATE_TYPE.into(),
            aggregate_nonce: nonce,
            event_type: event_type.into(),
            event_version: 1,
            content_type: "application/octet-stream".into(),
            tenant_id: TENANT.into(),
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
    let count: i64 = query_scalar("SELECT COUNT(*) FROM events")
        .fetch_one(store.pool())
        .await
        .expect("count events");
    assert_eq!(count, 0);

    let append_res = store
        .append(proto::AppendRequest {
            tenant_id: TENANT.into(),
            aggregate_id: AGGREGATE_ID.into(),
            aggregate_type: AGGREGATE_TYPE.into(),
            expected_aggregate_nonce: 0,
            idempotency_key: "batch-1".into(),
            events: vec![
                new_event(1, "00000000-0000-0000-0000-000000000001", "OrderSubmitted"),
                new_event(2, "00000000-0000-0000-0000-000000000002", "OrderConfirmed"),
            ],
        })
        .await
        .expect("append ok");
    assert_eq!(append_res.last_aggregate_nonce, 2);
    assert_eq!(append_res.last_global_nonce, 2);

    // Read forward
    let rs = store
        .read_stream(proto::ReadStreamRequest {
            tenant_id: TENANT.into(),
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
    assert_eq!(first_meta.tenant_id, TENANT);
    assert!(first_meta.global_nonce > 0);

    // Repeating append with identical idempotency key should short-circuit
    let replay_err = store
        .append(proto::AppendRequest {
            tenant_id: TENANT.into(),
            aggregate_id: AGGREGATE_ID.into(),
            aggregate_type: AGGREGATE_TYPE.into(),
            expected_aggregate_nonce: 2,
            idempotency_key: "batch-1".into(),
            events: vec![new_event(
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
            tenant_id: TENANT.into(),
            aggregate_id: AGGREGATE_ID.into(),
            aggregate_type: AGGREGATE_TYPE.into(),
            expected_aggregate_nonce: 1,
            idempotency_key: "batch-2".into(),
            events: vec![new_event(
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
            tenant_id: TENANT.into(),
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
                    tenant_id: TENANT.into(),
                    ..Default::default()
                }),
                payload: b"x".to_vec(),
            }],
        })
        .await
        .expect("append ok");

    let upd = query("UPDATE events SET event_type = 'Hacked' WHERE tenant_id = $1")
        .bind(TENANT)
        .execute(store.pool())
        .await;
    assert!(upd.is_err());

    let del = query("DELETE FROM events WHERE tenant_id = $1")
        .bind(TENANT)
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
            tenant_id: TENANT.into(),
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
                    tenant_id: TENANT.into(),
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
    .bind(TENANT)
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
            tenant_id: TENANT.into(),
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
                    tenant_id: TENANT.into(),
                    ..Default::default()
                }),
                payload: b"2".to_vec(),
            }],
        })
        .await
        .expect("append nonce 2");
    assert_eq!(res2.last_aggregate_nonce, 2);
}
