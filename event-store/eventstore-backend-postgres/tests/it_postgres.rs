use eventstore_backend_postgres::PostgresStore;
use sqlx::{query, query_scalar};
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres as PgImage;

use eventstore_core::proto::{self, append_request};
use eventstore_core::EventStore;

#[tokio::test]
async fn postgres_end_to_end_append_read_and_migrations() {
    // Spin up Postgres with default credentials (postgres/postgres)
    let container = PgImage::default().start().await.expect("start postgres");

    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("get mapped port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    // Connect and run migrations
    let store = PostgresStore::connect(&url).await.expect("connect+init");

    // Verify schema exists by simple count query
    let count: i64 = query_scalar("SELECT COUNT(*) FROM events")
        .fetch_one(store.pool())
        .await
        .expect("count events");
    assert_eq!(count, 0);

    // Append a couple events with NO_STREAM expectation
    let ev1 = proto::EventData {
        meta: Some(proto::EventMetadata {
            event_id: "00000000-0000-0000-0000-000000000001".into(),
            aggregate_id: "Order-1".into(),
            aggregate_type: "Order".into(),
            aggregate_nonce: 1, // First event in new aggregate
            event_type: "OrderSubmitted".into(),
            content_type: "application/octet-stream".into(),
            ..Default::default()
        }),
        payload: b"hello".to_vec(),
    };
    let ev2 = proto::EventData {
        meta: Some(proto::EventMetadata {
            event_id: "00000000-0000-0000-0000-000000000002".into(),
            aggregate_id: "Order-1".into(),
            aggregate_type: "Order".into(),
            aggregate_nonce: 2, // Second event in aggregate
            event_type: "OrderConfirmed".into(),
            content_type: "application/octet-stream".into(),
            ..Default::default()
        }),
        payload: b"world".to_vec(),
    };

    let append_res = store
        .append(proto::AppendRequest {
            aggregate_id: "Order-1".into(),
            aggregate_type: "Order".into(),
            expected: Some(append_request::Expected::ExpectedAny(
                proto::Expected::NoAggregate as i32,
            )),
            events: vec![ev1, ev2],
        })
        .await
        .expect("append ok");
    assert_eq!(append_res.next_aggregate_nonce, 2);

    // Read forward from 1, max 10
    let rs = store
        .read_stream(proto::ReadStreamRequest {
            aggregate_id: "Order-1".into(),
            from_aggregate_nonce: 1,
            max_count: 10,
            forward: true,
        })
        .await
        .expect("read ok");
    assert_eq!(rs.events.len(), 2);

    // Concurrency error: expecting exact 1 while stream is at 2
    let ev3 = proto::EventData {
        meta: Some(proto::EventMetadata {
            event_id: "00000000-0000-0000-0000-000000000003".into(),
            aggregate_id: "Order-1".into(),
            aggregate_type: "Order".into(),
            aggregate_nonce: 3, // Third event in aggregate
            event_type: "OrderShipped".into(),
            content_type: "application/octet-stream".into(),
            ..Default::default()
        }),
        payload: b"ship".to_vec(),
    };
    let append_err = store
        .append(proto::AppendRequest {
            aggregate_id: "Order-1".into(),
            aggregate_type: "Order".into(),
            expected: Some(append_request::Expected::Exact(1)),
            events: vec![ev3],
        })
        .await;
    assert!(append_err.is_err());

    // container dropped here, postgres stops
}

#[tokio::test]
async fn postgres_immutability_triggers_block_update_delete() {
    let container = PgImage::default().start().await.expect("start postgres");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("get mapped port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    let store = PostgresStore::connect(&url).await.expect("connect+init");

    // Append one event
    let ev = proto::EventData {
        meta: Some(proto::EventMetadata {
            event_id: "11111111-1111-1111-1111-111111111111".into(),
            aggregate_id: "Immut-1".into(),
            aggregate_type: "Immut".into(),
            aggregate_nonce: 1, // First event in new aggregate
            event_type: "Created".into(),
            content_type: "application/octet-stream".into(),
            ..Default::default()
        }),
        payload: b"x".to_vec(),
    };
    let _ = store
        .append(proto::AppendRequest {
            aggregate_id: "Immut-1".into(),
            aggregate_type: "Immut".into(),
            expected: Some(append_request::Expected::ExpectedAny(
                proto::Expected::NoAggregate as i32,
            )),
            events: vec![ev],
        })
        .await
        .expect("append ok");

    // Try to UPDATE -> should error due to trigger
    let upd = query("UPDATE events SET event_type = 'Hacked' WHERE aggregate_id = $1")
        .bind("Immut-1")
        .execute(store.pool())
        .await;
    assert!(upd.is_err(), "UPDATE should be blocked by trigger");

    // Try to DELETE -> should error due to trigger
    let del = query("DELETE FROM events WHERE aggregate_id = $1")
        .bind("Immut-1")
        .execute(store.pool())
        .await;
    assert!(del.is_err(), "DELETE should be blocked by trigger");
}

#[tokio::test]
async fn postgres_sequencing_trigger_enforces_prev_plus_one() {
    let container = PgImage::default().start().await.expect("start postgres");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("get mapped port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    let store = PostgresStore::connect(&url).await.expect("connect+init");

    // Append one event to establish version=1
    let ev1 = proto::EventData {
        meta: Some(proto::EventMetadata {
            event_id: "22222222-2222-2222-2222-222222222222".into(),
            aggregate_id: "Seq-1".into(),
            aggregate_type: "Seq".into(),
            aggregate_nonce: 1, // First event in new aggregate
            event_type: "E1".into(),
            content_type: "application/octet-stream".into(),
            ..Default::default()
        }),
        payload: b"1".to_vec(),
    };
    let _ = store
        .append(proto::AppendRequest {
            aggregate_id: "Seq-1".into(),
            aggregate_type: "Seq".into(),
            expected: Some(append_request::Expected::ExpectedAny(
                proto::Expected::NoAggregate as i32,
            )),
            events: vec![ev1],
        })
        .await
        .expect("append ok");

    // Attempt to skip to version 3 via raw SQL INSERT -> should fail by trigger
    let ins = query(
        r#"INSERT INTO events (
            event_id, aggregate_id, aggregate_type, stream_version, event_type, content_type, payload
        ) VALUES (
            $1::uuid, $2, $3, $4, $5, $6, $7
        )"#,
    )
    .bind("33333333-3333-3333-3333-333333333333")
    .bind("Seq-1")
    .bind("Seq")
    .bind(3_i64) // invalid: should be 2
    .bind(Some("E3".to_string()))
    .bind(Some("application/octet-stream".to_string()))
    .bind(b"3".to_vec())
    .execute(store.pool())
    .await;
    assert!(
        ins.is_err(),
        "INSERT skipping version should be blocked by trigger"
    );

    // Valid insert to version 2 via normal append should still work
    let ev2 = proto::EventData {
        meta: Some(proto::EventMetadata {
            event_id: "22222222-2222-2222-2222-222222222223".into(),
            aggregate_id: "Seq-1".into(),
            aggregate_type: "Seq".into(),
            aggregate_nonce: 2, // Second event in aggregate
            event_type: "E2".into(),
            content_type: "application/octet-stream".into(),
            ..Default::default()
        }),
        payload: b"2".to_vec(),
    };
    let res2 = store
        .append(proto::AppendRequest {
            aggregate_id: "Seq-1".into(),
            aggregate_type: "Seq".into(),
            expected: Some(append_request::Expected::Exact(1)),
            events: vec![ev2],
        })
        .await
        .expect("append v2 ok");
    assert_eq!(res2.next_aggregate_nonce, 2);
}
