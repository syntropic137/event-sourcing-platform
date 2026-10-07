//! Historical event-version fixtures (tests/fixtures/historical-events.json).

use std::collections::{BTreeMap, HashMap};

use serde::Deserialize;

use super::projection::AccountState;
use super::workload::{event, Cmd, AGGREGATE_TYPE};
use eventstore_proto::gen::AppendRequest;

const RAW: &str = include_str!("../fixtures/historical-events.json");

#[derive(Deserialize)]
struct File {
    append_order: Vec<(String, usize)>,
    streams: BTreeMap<String, Vec<FixtureEvent>>,
    expected: BTreeMap<String, Expected>,
}

#[derive(Deserialize)]
struct FixtureEvent {
    event_type: String,
    event_version: u32,
    content_type: String,
    payload: String,
    headers: HashMap<String, String>,
}

#[derive(Deserialize)]
struct Expected {
    owner: String,
    currency: String,
    balance_minor: i64,
    events_applied: i64,
}

/// One command per fixture event, in the fixture's interleaved append order,
/// plus the projection state an upcasting consumer must reach.
pub fn historical(tenant: &str) -> (Vec<Cmd>, BTreeMap<String, AccountState>) {
    let file: File = serde_json::from_str(RAW).expect("parse fixture file");
    let total: usize = file.streams.values().map(Vec::len).sum();
    assert_eq!(
        file.append_order.len(),
        total,
        "append_order covers every event"
    );
    let mut cmds = Vec::new();
    for (stream, idx) in &file.append_order {
        let fx = &file.streams[stream][*idx];
        let nonce = *idx as u64 + 1;
        let mut ev = event(
            tenant,
            stream,
            nonce,
            &fx.event_type,
            fx.event_version,
            fx.payload.as_bytes().to_vec(),
        );
        let meta = ev.meta.as_mut().unwrap();
        meta.content_type = fx.content_type.clone();
        meta.headers = fx.headers.clone();
        cmds.push(Cmd {
            req: AppendRequest {
                tenant_id: tenant.into(),
                aggregate_id: stream.clone(),
                aggregate_type: AGGREGATE_TYPE.into(),
                expected_aggregate_nonce: nonce - 1,
                idempotency_key: format!("fixture/{stream}/{nonce}"),
                events: vec![ev],
            },
        });
    }
    let expected = file
        .expected
        .into_iter()
        .map(|(k, e)| {
            (
                k,
                AccountState {
                    owner: e.owner,
                    currency: e.currency,
                    balance_minor: e.balance_minor,
                    events_applied: e.events_applied,
                },
            )
        })
        .collect();
    (cmds, expected)
}
