//! Golden cross-language tests (ADR-026).
//!
//! `tests/fixtures/xlang/{typescript,python}.json` hold the protobuf
//! `AppendRequest` bytes produced by the real TypeScript and Python SDK
//! encoders (regenerate with `make test-xlang-fixtures`). These tests:
//!
//! 1. decode the fixture events with the Rust SDK (wire level and through a
//!    repository backed by a live in-process event store), and
//! 2. encode the same events with the Rust repository and require
//!    byte-identical metadata and JSON-equal payloads.

mod common;

use std::sync::Arc;

use common::xlang::{
    base64_decode, fixture_events, fixtures_dir, Account, AccountEvent, CapturePort,
    MoneyDeposited, NOTE,
};
use common::{connect, spawn_server, unique_tenant};
use event_sourcing_rust::client::{proto, EventStorePort};
use event_sourcing_rust::prelude::*;
use event_sourcing_rust::projection::RecordedEvent;
use prost::Message;
use serde_json::Value;

struct Fixture {
    producer: String,
    request: proto::AppendRequest,
}

fn load(name: &str) -> Fixture {
    let path = fixtures_dir().join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    let json: Value = serde_json::from_str(&text).unwrap();
    let bytes = base64_decode(json["append_request_base64"].as_str().unwrap());
    let request = proto::AppendRequest::decode(bytes.as_slice()).expect("protobuf AppendRequest");
    // The human-readable copy must describe the same bytes.
    let payloads: Vec<&str> = json["payloads"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap())
        .collect();
    let wire: Vec<&str> = request
        .events
        .iter()
        .map(|e| std::str::from_utf8(&e.payload).unwrap())
        .collect();
    assert_eq!(payloads, wire, "{name}: payloads field out of sync");
    Fixture {
        producer: json["producer"].as_str().unwrap().to_string(),
        request,
    }
}

fn fixtures() -> Vec<Fixture> {
    let all = vec![load("typescript"), load("python")];
    assert_eq!(all[0].producer, "typescript");
    assert_eq!(all[1].producer, "python");
    all
}

/// Events a producer writes: Python cannot write v2 yet (ADR-026).
fn expected(f: &Fixture) -> Vec<AccountEvent> {
    let mut events = fixture_events(&f.request.aggregate_id);
    events.truncate(f.request.events.len());
    events
}

fn recorded(f: &Fixture) -> Vec<RecordedEvent> {
    f.request
        .events
        .iter()
        .cloned()
        .map(|e| RecordedEvent::from_proto(e).unwrap())
        .collect()
}

#[test]
fn fixtures_follow_the_envelope() {
    for f in fixtures() {
        let r = &f.request;
        assert_eq!(r.aggregate_type, "Account", "{}", f.producer);
        assert_eq!(r.expected_aggregate_nonce, 0);
        for (i, e) in r.events.iter().enumerate() {
            let m = e.meta.as_ref().unwrap();
            assert_eq!(m.aggregate_type, "Account");
            assert_eq!(m.aggregate_id, r.aggregate_id);
            assert_eq!(m.aggregate_nonce, i as u64 + 1);
            assert_eq!(m.content_type, "application/json");
            assert_eq!(m.tenant_id, r.tenant_id);
            assert!(m.event_version >= 1);
            let body: Value = serde_json::from_slice(&e.payload).unwrap();
            assert!(body.is_object(), "{}: payload is a JSON object", f.producer);
            assert!(
                body.get(&m.event_type).is_none(),
                "{}: payload is not wrapped in a type tag",
                f.producer
            );
        }
    }
}

#[test]
fn rust_decodes_typescript_and_python_events() {
    for f in fixtures() {
        let decoded: Vec<AccountEvent> = recorded(&f)
            .iter()
            .map(|e| {
                e.decode()
                    .unwrap_or_else(|err| panic!("{}: {err}", f.producer))
            })
            .collect();
        assert_eq!(decoded, expected(&f), "{}", f.producer);
    }
}

#[test]
fn rust_upcasts_v1_fixture_events_to_v2() {
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct MoneyDepositedV2 {
        amount: i64,
        note: String,
        currency: String,
    }
    impl EventSchema for MoneyDepositedV2 {
        const EVENT_TYPE: &'static str = "MoneyDeposited";
        const EVENT_VERSION: u32 = 2;
    }
    let upcasters = Upcasters::new().register("MoneyDeposited", 1, 2, |mut body| {
        body["currency"] = "EUR".into();
        Ok(body)
    });
    for f in fixtures() {
        let deposit = &recorded(&f)[1];
        let v2: MoneyDepositedV2 = deposit.decode_with(&upcasters).unwrap();
        assert_eq!(
            v2,
            MoneyDepositedV2 {
                amount: 125,
                note: NOTE.into(),
                currency: "EUR".into()
            },
            "{}",
            f.producer
        );
        // Without the upcaster the v2 schema rejects a v1 event.
        assert!(matches!(
            deposit.decode::<MoneyDepositedV2>(),
            Err(Error::UnknownEventVersion { .. })
        ));
        assert_eq!(
            deposit.decode::<MoneyDeposited>().unwrap().note,
            NOTE,
            "{}",
            f.producer
        );
    }
}

#[tokio::test]
async fn rust_repository_loads_fixture_streams() {
    let server = spawn_server().await;
    let store: Arc<dyn EventStorePort> = Arc::new(connect(&server.addr).await);
    for f in fixtures() {
        // Replay the producer's exact AppendRequest into a live store.
        let tenant = unique_tenant();
        let mut request = f.request.clone();
        request.tenant_id = tenant.clone();
        for e in &mut request.events {
            e.meta.as_mut().unwrap().tenant_id = tenant.clone();
        }
        store.append(request).await.unwrap();

        let repo = EventStoreRepository::<Account>::new(store.clone(), &tenant);
        let loaded = repo
            .load(&f.request.aggregate_id)
            .await
            .unwrap()
            .expect("stream exists");
        assert_eq!(loaded.aggregate.events, expected(&f), "{}", f.producer);
        assert_eq!(loaded.committed_version(), f.request.events.len() as u64);
    }
}

/// Keys the TypeScript SDK echoes from its event class into the payload
/// (known deviation, ADR-026). Not part of the canonical body.
const TS_ECHO_KEYS: [&str; 2] = ["eventType", "schemaVersion"];

#[tokio::test]
async fn rust_encoding_matches_typescript_and_python() {
    for f in fixtures() {
        let port = Arc::new(CapturePort::default());
        let repo = EventStoreRepository::<Account>::new(port.clone(), &f.request.tenant_id);
        let mut instance =
            AggregateInstance::new(f.request.aggregate_id.clone(), Account::default());
        instance.add_events(expected(&f)).unwrap();
        // Event id and client time are per-write values; take the fixture's.
        for (env, theirs) in instance
            .uncommitted_events
            .iter_mut()
            .zip(&f.request.events)
        {
            let m = theirs.meta.as_ref().unwrap();
            env.metadata.event_id = Uuid::parse_str(&m.event_id).unwrap();
            env.metadata.timestamp =
                DateTime::<Utc>::from_timestamp_millis(m.timestamp_unix_ms as i64).unwrap();
        }
        repo.save(&mut instance).await.unwrap();

        let ours = port.appends.lock().unwrap().remove(0);
        assert_eq!(ours.tenant_id, f.request.tenant_id, "{}", f.producer);
        assert_eq!(ours.aggregate_id, f.request.aggregate_id);
        assert_eq!(ours.aggregate_type, f.request.aggregate_type);
        assert_eq!(
            ours.expected_aggregate_nonce,
            f.request.expected_aggregate_nonce
        );
        assert_eq!(ours.events.len(), f.request.events.len());
        for (mine, theirs) in ours.events.iter().zip(&f.request.events) {
            let (mm, tm) = (mine.meta.as_ref().unwrap(), theirs.meta.as_ref().unwrap());
            assert_eq!(mm, tm, "{}: metadata", f.producer);
            assert_eq!(
                mm.encode_to_vec(),
                tm.encode_to_vec(),
                "{}: metadata bytes",
                f.producer
            );

            let mine: Value = serde_json::from_slice(&mine.payload).unwrap();
            let mut theirs: Value = serde_json::from_slice(&theirs.payload).unwrap();
            if f.producer == "typescript" {
                for key in TS_ECHO_KEYS {
                    theirs.as_object_mut().unwrap().remove(key);
                }
            }
            assert_eq!(mine, theirs, "{}: payload", f.producer);
        }
    }
}
