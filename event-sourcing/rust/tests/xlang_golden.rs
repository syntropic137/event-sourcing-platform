//! Golden cross-language tests (ADR-027).
//!
//! `tests/fixtures/xlang/*.json` hold protobuf `AppendRequest` bytes:
//!
//! * `typescript.json`, `python.json`: from the real TypeScript and Python
//!   SDK encoders (regenerate with `make test-xlang-fixtures`);
//! * `rust.json`: from the Rust repository (checked by
//!   `rust_fixture_is_current`; regenerate with `make test-xlang-fixtures`);
//! * `typescript-legacy.json`: frozen output of the TypeScript SDK 0.17
//!   encoder, whose payloads echo `eventType`/`schemaVersion`. Already-stored
//!   streams look like this, so every reader must keep decoding it.
//!
//! These tests decode every fixture with the Rust SDK (wire level and through
//! a repository backed by a live in-process event store), and encode the same
//! events with the Rust repository, requiring byte-identical metadata and
//! JSON-equal payloads. The TypeScript and Python SDK test suites decode the
//! same fixtures (`tests/xlang-golden.test.ts`, `tests/unit/test_xlang_golden.py`).

mod common;

use std::sync::Arc;

use common::xlang::{
    base64_decode, base64_encode, fixture_events, fixtures_dir, Account, AccountEvent, CapturePort,
    MoneyDeposited, NOTE,
};
use common::{connect, spawn_server, unique_tenant};
use event_sourcing_rust::client::{proto, EventStorePort};
use event_sourcing_rust::prelude::*;
use event_sourcing_rust::projection::RecordedEvent;
use prost::Message;
use serde_json::{json, Value};

/// Fixtures produced by the current encoders of each SDK.
const CANONICAL: [&str; 3] = ["typescript", "python", "rust"];
/// Every fixture, including already-stored legacy shapes.
const ALL: [&str; 4] = ["typescript", "python", "rust", "typescript-legacy"];

/// Keys TypeScript SDK <= 0.17 echoed from its event class into the payload.
const TS_ECHO_KEYS: [&str; 2] = ["eventType", "schemaVersion"];

const RUST_AGGREGATE_ID: &str = "acct-rs-1";
const GOLDEN_TENANT: &str = "tenant-golden";
const GOLDEN_TIME_MS: i64 = 1_767_323_045_678;

struct Fixture {
    producer: String,
    json: Value,
    request: proto::AppendRequest,
}

fn load(name: &str) -> Fixture {
    let path = fixtures_dir().join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    let json: Value = serde_json::from_str(&text).unwrap();
    let bytes = base64_decode(json["append_request_base64"].as_str().unwrap());
    let request = proto::AppendRequest::decode(bytes.as_slice()).expect("protobuf AppendRequest");
    let fixture = Fixture {
        producer: json["producer"].as_str().unwrap().to_string(),
        json,
        request,
    };
    assert_eq!(fixture.producer, name);
    fixture
}

fn fixtures(names: &[&str]) -> Vec<Fixture> {
    names.iter().map(|n| load(n)).collect()
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
fn readable_copies_describe_the_same_bytes() {
    // The TS and Python golden tests read `append_request` (proto3 JSON) and
    // `payloads`; both must match the protobuf bytes exactly.
    for f in fixtures(&ALL) {
        let payloads: Vec<&str> = f.json["payloads"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap())
            .collect();
        let wire: Vec<&str> = f
            .request
            .events
            .iter()
            .map(|e| std::str::from_utf8(&e.payload).unwrap())
            .collect();
        assert_eq!(payloads, wire, "{}: payloads field out of sync", f.producer);
        assert_eq!(
            normalize_proto_json(&f.json["append_request"]),
            normalize_proto_json(&append_request_json(&f.request)),
            "{}: append_request field out of sync",
            f.producer
        );
    }
}

#[test]
fn fixtures_follow_the_envelope() {
    for f in fixtures(&ALL) {
        let r = &f.request;
        assert_eq!(r.aggregate_type, "Account", "{}", f.producer);
        assert_eq!(r.expected_aggregate_nonce, 0);
        assert_eq!(
            r.events.len(),
            3,
            "{}: every SDK writes all three",
            f.producer
        );
        for (i, e) in r.events.iter().enumerate() {
            let m = e.meta.as_ref().unwrap();
            assert_eq!(m.aggregate_type, "Account");
            assert_eq!(m.aggregate_id, r.aggregate_id);
            assert_eq!(m.aggregate_nonce, i as u64 + 1);
            assert_eq!(m.content_type, "application/json");
            assert_eq!(m.tenant_id, r.tenant_id);
            let body: Value = serde_json::from_slice(&e.payload).unwrap();
            assert!(body.is_object(), "{}: payload is a JSON object", f.producer);
            assert!(
                body.get(&m.event_type).is_none(),
                "{}: payload is not wrapped in a type tag",
                f.producer
            );
        }
        // Real versions on the wire, including Python's v2 (#382).
        let versions: Vec<u32> = r
            .events
            .iter()
            .map(|e| e.meta.as_ref().unwrap().event_version)
            .collect();
        assert_eq!(versions, [1, 1, 2], "{}", f.producer);
    }
}

#[test]
fn canonical_payloads_hold_only_event_fields() {
    for f in fixtures(&CANONICAL) {
        for (e, want) in f
            .request
            .events
            .iter()
            .zip(fixture_events(&f.request.aggregate_id))
        {
            let body: Value = serde_json::from_slice(&e.payload).unwrap();
            let want: Value = serde_json::from_slice(&want.to_payload().unwrap()).unwrap();
            assert_eq!(body, want, "{}: exactly the event's fields", f.producer);
        }
    }
}

#[test]
fn rust_decodes_every_fixture() {
    for f in fixtures(&ALL) {
        let decoded: Vec<AccountEvent> = recorded(&f)
            .iter()
            .map(|e| {
                e.decode()
                    .unwrap_or_else(|err| panic!("{}: {err}", f.producer))
            })
            .collect();
        assert_eq!(
            decoded,
            fixture_events(&f.request.aggregate_id),
            "{}",
            f.producer
        );
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
    for f in fixtures(&ALL) {
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
    for f in fixtures(&ALL) {
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
        assert_eq!(
            loaded.aggregate.events,
            fixture_events(&f.request.aggregate_id),
            "{}",
            f.producer
        );
        assert_eq!(loaded.committed_version(), f.request.events.len() as u64);
    }
}

/// What the Rust repository writes for the fixture events, with the given
/// per-write values (event id, client time).
async fn rust_encode(
    tenant: &str,
    aggregate_id: &str,
    ids_and_times: &[(Uuid, i64)],
) -> proto::AppendRequest {
    let port = Arc::new(CapturePort::default());
    let repo = EventStoreRepository::<Account>::new(port.clone(), tenant);
    let mut instance = AggregateInstance::new(aggregate_id.to_string(), Account::default());
    instance.add_events(fixture_events(aggregate_id)).unwrap();
    for (env, (id, ms)) in instance.uncommitted_events.iter_mut().zip(ids_and_times) {
        env.metadata.event_id = *id;
        env.metadata.timestamp = DateTime::<Utc>::from_timestamp_millis(*ms).unwrap();
    }
    repo.save(&mut instance).await.unwrap();
    let ours = port.appends.lock().unwrap().remove(0);
    ours
}

#[tokio::test]
async fn rust_encoding_matches_typescript_and_python() {
    for f in fixtures(&["typescript", "python", "typescript-legacy"]) {
        let per_write: Vec<(Uuid, i64)> = f
            .request
            .events
            .iter()
            .map(|e| {
                let m = e.meta.as_ref().unwrap();
                (
                    Uuid::parse_str(&m.event_id).unwrap(),
                    m.timestamp_unix_ms as i64,
                )
            })
            .collect();
        let ours = rust_encode(&f.request.tenant_id, &f.request.aggregate_id, &per_write).await;
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
            if f.producer == "typescript-legacy" {
                // The only difference legacy TS payloads have (ADR-027).
                for key in TS_ECHO_KEYS {
                    assert!(theirs.as_object_mut().unwrap().remove(key).is_some());
                }
            }
            assert_eq!(mine, theirs, "{}: payload", f.producer);
        }
    }
}

fn rust_fixture_ids() -> Vec<(Uuid, i64)> {
    (1..=3)
        .map(|n| {
            let id = format!("00000000-0000-4000-8000-0000000002{n:02}");
            (Uuid::parse_str(&id).unwrap(), GOLDEN_TIME_MS)
        })
        .collect()
}

/// `tests/fixtures/xlang/rust.json` is what the Rust encoder writes today.
/// `XLANG_UPDATE_FIXTURES=1` rewrites it (`make test-xlang-fixtures`).
#[tokio::test]
async fn rust_fixture_is_current() {
    let request = rust_encode(GOLDEN_TENANT, RUST_AGGREGATE_ID, &rust_fixture_ids()).await;
    let bytes = request.encode_to_vec();
    let fixture = json!({
        "producer": "rust",
        "note": "Generated by event-sourcing/rust/tests/xlang_golden.rs (XLANG_UPDATE_FIXTURES=1). Do not edit.",
        "append_request_base64": base64_encode(&bytes),
        "append_request": append_request_json(&request),
        "payloads": request
            .events
            .iter()
            .map(|e| String::from_utf8(e.payload.clone()).unwrap())
            .collect::<Vec<_>>(),
    });
    let text = serde_json::to_string_pretty(&fixture).unwrap() + "\n";
    let path = fixtures_dir().join("rust.json");
    if std::env::var("XLANG_UPDATE_FIXTURES").as_deref() == Ok("1") {
        std::fs::write(&path, &text).unwrap();
    }
    let stored = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{path:?}: {e}; run `make test-xlang-fixtures`"));
    assert_eq!(
        stored, text,
        "rust.json is stale: the Rust encoder changed. Review the diff, then run \
         `make test-xlang-fixtures` (the envelope is a one-way door)"
    );
}

/// Proto3 JSON mapping of an `AppendRequest` (lowerCamelCase names, 64-bit
/// integers as strings, default values omitted), as `MessageToDict` writes it.
fn append_request_json(r: &proto::AppendRequest) -> Value {
    let mut out = serde_json::Map::new();
    put_str(&mut out, "tenantId", &r.tenant_id);
    put_str(&mut out, "aggregateId", &r.aggregate_id);
    put_str(&mut out, "aggregateType", &r.aggregate_type);
    put_u64(
        &mut out,
        "expectedAggregateNonce",
        r.expected_aggregate_nonce,
    );
    put_str(&mut out, "idempotencyKey", &r.idempotency_key);
    let events: Vec<Value> = r
        .events
        .iter()
        .map(|e| {
            let m = e.meta.as_ref().unwrap();
            let mut meta = serde_json::Map::new();
            put_str(&mut meta, "eventId", &m.event_id);
            put_str(&mut meta, "aggregateId", &m.aggregate_id);
            put_str(&mut meta, "aggregateType", &m.aggregate_type);
            put_u64(&mut meta, "aggregateNonce", m.aggregate_nonce);
            put_str(&mut meta, "eventType", &m.event_type);
            if m.event_version != 0 {
                meta.insert("eventVersion".into(), json!(m.event_version));
            }
            put_str(&mut meta, "contentType", &m.content_type);
            put_str(&mut meta, "contentSchema", &m.content_schema);
            put_str(&mut meta, "correlationId", &m.correlation_id);
            put_str(&mut meta, "causationId", &m.causation_id);
            put_str(&mut meta, "actorId", &m.actor_id);
            put_str(&mut meta, "tenantId", &m.tenant_id);
            put_u64(&mut meta, "timestampUnixMs", m.timestamp_unix_ms);
            put_u64(&mut meta, "recordedTimeUnixMs", m.recorded_time_unix_ms);
            if !m.payload_sha256.is_empty() {
                meta.insert(
                    "payloadSha256".into(),
                    json!(base64_encode(&m.payload_sha256)),
                );
            }
            if !m.headers.is_empty() {
                meta.insert("headers".into(), json!(m.headers));
            }
            put_u64(&mut meta, "globalNonce", m.global_nonce);
            json!({ "meta": meta, "payload": base64_encode(&e.payload) })
        })
        .collect();
    out.insert("events".into(), Value::Array(events));
    Value::Object(out)
}

fn put_str(map: &mut serde_json::Map<String, Value>, key: &str, value: &str) {
    if !value.is_empty() {
        map.insert(key.into(), json!(value));
    }
}

fn put_u64(map: &mut serde_json::Map<String, Value>, key: &str, value: u64) {
    if value != 0 {
        map.insert(key.into(), json!(value.to_string()));
    }
}

/// Producers differ only in how they print 64-bit integers (TS numbers,
/// Python strings) and whether default values appear; compare the values.
fn normalize_proto_json(v: &Value) -> Value {
    match v {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(_, v)| !is_default(v))
                .map(|(k, v)| (k.clone(), normalize_proto_json(v)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(normalize_proto_json).collect()),
        Value::Number(n) => Value::String(n.to_string()),
        other => other.clone(),
    }
}

fn is_default(v: &Value) -> bool {
    match v {
        Value::String(s) => s.is_empty() || s == "0",
        Value::Number(n) => n.as_u64() == Some(0),
        Value::Object(m) => m.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Bool(b) => !b,
        Value::Null => true,
    }
}
