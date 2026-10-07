//! `SubscribeRequest.aggregate_id_prefix` is matched literally (#361).
//!
//! Before the fix the Postgres backend used the prefix as an unescaped SQL
//! LIKE pattern: `_` and `%` matched any character(s) and `\` escaped the
//! next one, so a prefix could deliver foreign aggregates or miss its own.

mod common;

use std::sync::Arc;
use std::time::Duration;

use eventstore_backend_postgres::PostgresStore;
use eventstore_core::{proto, EventStore, StoreError, StoreStream};
use futures::StreamExt;

const STEP: Duration = Duration::from_secs(15);

fn unique_tenant(name: &str) -> String {
    let run = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("tenant-prefix-{name}-{run}")
}

async fn connect() -> Arc<PostgresStore> {
    let url = common::get_test_database_url().await;
    PostgresStore::connect_for_tests(&url)
        .await
        .expect("connect")
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

/// Subscribing with `prefix` delivers exactly the events of aggregates whose
/// id starts with it (byte for byte), in order.
async fn assert_literal_prefix(store: &PostgresStore, tenant: &str, prefix: &str) {
    let all = until_caught_up(&mut store.subscribe(request(tenant, "", 0))).await;
    let expected: Vec<_> = all
        .iter()
        .filter(|(agg, _)| agg.starts_with(prefix))
        .cloned()
        .collect();
    assert!(!expected.is_empty(), "fixture must match '{prefix}'");
    let got = until_caught_up(&mut store.subscribe(request(tenant, prefix, 0))).await;
    assert_eq!(got, expected, "prefix '{prefix}' must match literally");
}

#[tokio::test]
async fn prefix_wildcard_characters_match_literally() {
    let tenant = unique_tenant("literal");
    let store = connect().await;
    // Each pair: an id with the literal character, and one an unescaped
    // LIKE would (or would not) match instead.
    interleave(
        &store,
        &tenant,
        &[
            "acct_1",
            "acctX1", // `_` is any one character in LIKE
            "acct%1",
            "acct-long-1", // `%` is any run in LIKE
            r"acct\1",
            r"acct\\1",
            "acct1", // `\` escapes the next one in LIKE
            "acct",  // a prefix of the prefixes
        ],
        2,
    )
    .await;

    for prefix in ["acct_", "acct%", r"acct\", r"acct\\", "acct", "acct_1"] {
        assert_literal_prefix(&store, &tenant, prefix).await;
    }
}

#[tokio::test]
async fn literal_prefix_also_applies_to_live_delivery() {
    let tenant = unique_tenant("literal-live");
    let store = connect().await;
    let mut stream = store.subscribe(request(&tenant, "a_", 0));
    assert!(until_caught_up(&mut stream).await.is_empty());

    append(&store, &tenant, "aX1", 0, 1).await; // matches unescaped LIKE only
    let wanted = append(&store, &tenant, "a_1", 0, 1).await;
    loop {
        match next(&mut stream).await {
            Some(Ok(proto::SubscribeResponse { event: None })) => continue,
            Some(Ok(proto::SubscribeResponse { event: Some(ev) })) => {
                let meta = ev.meta.expect("meta");
                assert_eq!(
                    (meta.aggregate_id.as_str(), meta.global_nonce),
                    ("a_1", wanted[0]),
                    "'aX1' must not match prefix 'a_'"
                );
                break;
            }
            other => panic!("expected the matching live event, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn backend_advertises_literal_subscription_prefix() {
    let store = connect().await;
    assert!(store
        .capabilities()
        .contains(&eventstore_core::capabilities::LITERAL_SUBSCRIPTION_PREFIX));
}
