//! Append semantics the single-statement write path (#370) must keep: one
//! round trip writes all events, the stream head and the idempotency record
//! while the append-order lock is held.
mod common;

use eventstore_backend_postgres::PostgresStore;
use eventstore_core::{proto, EventStore, StoreError};
use std::collections::HashMap;

fn unique(prefix: &str) -> String {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{prefix}-{n}")
}

fn event(tenant: &str, agg: &str, nonce: u64, i: usize) -> proto::EventData {
    let mut headers = HashMap::new();
    if i.is_multiple_of(2) {
        headers.insert("k".to_string(), format!("v{i}"));
    }
    proto::EventData {
        meta: Some(proto::EventMetadata {
            event_id: format!("{agg}-ev-{nonce}"),
            aggregate_id: agg.into(),
            aggregate_type: "Batch".into(),
            aggregate_nonce: nonce,
            event_type: format!("T{i}"),
            event_version: (i as u32) + 1,
            content_type: if i.is_multiple_of(3) {
                String::new()
            } else {
                "application/json".into()
            },
            content_schema: if i % 2 == 1 {
                format!("s{i}")
            } else {
                String::new()
            },
            correlation_id: if i.is_multiple_of(2) {
                format!("c{i}")
            } else {
                String::new()
            },
            causation_id: if i % 3 == 1 {
                format!("z{i}")
            } else {
                String::new()
            },
            actor_id: if i.is_multiple_of(4) {
                format!("a{i}")
            } else {
                String::new()
            },
            tenant_id: tenant.into(),
            timestamp_unix_ms: 1_700_000_000_000 + i as u64,
            payload_sha256: if i.is_multiple_of(2) {
                vec![i as u8; 32]
            } else {
                vec![]
            },
            headers,
            ..Default::default()
        }),
        payload: if i % 5 == 4 {
            vec![]
        } else {
            format!("p{i}").into_bytes()
        },
    }
}

fn req(tenant: &str, agg: &str, from: u64, n: usize, key: &str) -> proto::AppendRequest {
    proto::AppendRequest {
        tenant_id: tenant.into(),
        aggregate_id: agg.into(),
        aggregate_type: "Batch".into(),
        expected_aggregate_nonce: from - 1,
        idempotency_key: key.into(),
        events: (0..n)
            .map(|i| event(tenant, agg, from + i as u64, i))
            .collect(),
    }
}

async fn store() -> std::sync::Arc<PostgresStore> {
    let url = common::get_test_database_url().await;
    PostgresStore::connect_for_tests(&url).await.unwrap()
}

#[tokio::test]
async fn batch_round_trips_every_field_in_aggregate_and_global_order() {
    let store = store().await;
    let tenant = unique("t-batch");
    let first = store
        .append(req(&tenant, "agg-1", 1, 7, "k1"))
        .await
        .unwrap();
    let second = store.append(req(&tenant, "agg-1", 8, 3, "")).await.unwrap();
    assert_eq!(first.last_aggregate_nonce, 7);
    assert_eq!(second.last_aggregate_nonce, 10);
    assert!(second.last_global_nonce > first.last_global_nonce);

    let read = store
        .read_stream(proto::ReadStreamRequest {
            tenant_id: tenant.clone(),
            aggregate_id: "agg-1".into(),
            from_aggregate_nonce: 1,
            max_count: 100,
            forward: true,
        })
        .await
        .unwrap();
    assert_eq!(read.events.len(), 10);
    let mut prev_global = 0;
    for (idx, got) in read.events.iter().enumerate() {
        let (from, i) = if idx < 7 { (1, idx) } else { (8, idx - 7) };
        let want = event(&tenant, "agg-1", from + i as u64, i);
        let g = got.meta.as_ref().unwrap();
        let w = want.meta.as_ref().unwrap();
        assert_eq!(g.aggregate_nonce, idx as u64 + 1);
        assert!(
            g.global_nonce > prev_global,
            "global order follows aggregate order"
        );
        prev_global = g.global_nonce;
        assert_eq!(g.event_id, w.event_id);
        assert_eq!(g.event_type, w.event_type);
        assert_eq!(g.event_version, w.event_version);
        let ct = if w.content_type.is_empty() {
            "application/octet-stream"
        } else {
            &w.content_type
        };
        assert_eq!(g.content_type, ct);
        assert_eq!(g.content_schema, w.content_schema);
        assert_eq!(g.correlation_id, w.correlation_id);
        assert_eq!(g.causation_id, w.causation_id);
        assert_eq!(g.actor_id, w.actor_id);
        assert_eq!(g.timestamp_unix_ms, w.timestamp_unix_ms);
        assert_eq!(g.payload_sha256, w.payload_sha256);
        assert_eq!(g.headers, w.headers);
        assert!(g.recorded_time_unix_ms > 0);
        assert_eq!(got.payload, want.payload);
    }
    // Batch boundaries are contiguous in global order within one append.
    assert_eq!(
        read.events[6].meta.as_ref().unwrap().global_nonce,
        first.last_global_nonce
    );
    assert_eq!(
        read.events[9].meta.as_ref().unwrap().global_nonce,
        second.last_global_nonce
    );
}

#[tokio::test]
async fn keyed_batch_retry_returns_the_recorded_result_and_writes_nothing() {
    let store = store().await;
    let tenant = unique("t-batch-idem");
    let r = req(&tenant, "agg", 1, 5, "key-1");
    let first = store.append(r.clone()).await.unwrap();
    let again = store.append(r.clone()).await.unwrap();
    assert_eq!(again.last_global_nonce, first.last_global_nonce);
    assert_eq!(again.last_aggregate_nonce, 5);

    let mut changed = r.clone();
    changed.events[2].payload = b"different".to_vec();
    match store.append(changed).await {
        Err(StoreError::AlreadyExists(_)) => {}
        other => panic!("same key, other content must be ALREADY_EXISTS: {other:?}"),
    }
    let all = store
        .read_all(proto::ReadAllRequest {
            tenant_id: tenant.clone(),
            from_global_nonce: 0,
            max_count: 100,
            forward: true,
        })
        .await
        .unwrap();
    assert_eq!(all.events.len(), 5, "written exactly once");
}

#[tokio::test]
async fn a_failing_row_rolls_back_the_whole_batch() {
    let store = store().await;
    let tenant = unique("t-batch-fail");
    // Event 3 reuses event 1's id: unique (tenant_id, event_id) fails.
    let mut r = req(&tenant, "agg", 1, 4, "key-x");
    r.events[2].meta.as_mut().unwrap().event_id =
        r.events[0].meta.as_ref().unwrap().event_id.clone();
    match store.append(r).await {
        Err(StoreError::Concurrency { .. }) => {}
        other => panic!("duplicate event id must fail the append: {other:?}"),
    }
    let all = store
        .read_all(proto::ReadAllRequest {
            tenant_id: tenant.clone(),
            from_global_nonce: 0,
            max_count: 100,
            forward: true,
        })
        .await
        .unwrap();
    assert!(all.events.is_empty(), "nothing partial is visible");
    // Neither the head nor the idempotency record moved: a valid append at
    // nonce 1 with the same key now succeeds.
    let ok = store
        .append(req(&tenant, "agg", 1, 2, "key-x"))
        .await
        .unwrap();
    assert_eq!(ok.last_aggregate_nonce, 2);
}

/// Before #370 a failing `pg_notify` was logged as "non-fatal", but the
/// error had already aborted the transaction: COMMIT then rolled back and
/// the append reported success for events that were never stored.
#[tokio::test]
async fn an_append_too_long_to_notify_is_stored_and_reported() {
    let store = store().await;
    // A NOTIFY payload must be under 8000 bytes; this tenant id is longer.
    // (It compresses, so the index entries still fit.) The append must
    // succeed (NOTIFY is skipped, subscribers poll) and be stored.
    let tenant = format!("{}{}", unique("t-long"), "x".repeat(8100));
    let ack = store
        .append(req(&tenant, "agg", 1, 1, ""))
        .await
        .expect("a valid append must not fail because NOTIFY cannot carry it");
    let read = store
        .read_stream(proto::ReadStreamRequest {
            tenant_id: tenant.clone(),
            aggregate_id: "agg".into(),
            from_aggregate_nonce: 1,
            max_count: 10,
            forward: true,
        })
        .await
        .unwrap();
    assert_eq!(read.events.len(), 1, "acknowledged append must be stored");
    assert_eq!(
        read.events[0].meta.as_ref().unwrap().global_nonce,
        ack.last_global_nonce
    );
}
