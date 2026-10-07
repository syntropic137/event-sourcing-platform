//! Live cross-language round trips against a real event store (ADR-027):
//! TypeScript and Python write, Rust reads; Rust writes, TypeScript and
//! Python read. Uses the real SDKs through `tests/xlang/ts_peer.cjs` and
//! `tests/xlang/py_peer.py`.
//!
//! Needs `node` (after `pnpm install` at the repo root) and `uv`, so the tests
//! are ignored by default. Run with `make test-xlang`.

mod common;

use std::path::PathBuf;
use std::sync::Arc;

use common::xlang::{fixture_events, xlang_dir, Account, AccountEvent};
use common::{connect, spawn_server, unique_tenant};
use event_sourcing_rust::client::EventStorePort;
use event_sourcing_rust::prelude::*;
use serde_json::{json, Value};
use tokio::process::Command;

fn python_sdk() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../python")
}

async fn run(mut cmd: Command) -> String {
    let out = cmd.output().await.expect("spawn peer");
    assert!(
        out.status.success(),
        "peer failed: {}\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

async fn ts(args: &[&str]) -> String {
    let mut cmd = Command::new("node");
    cmd.arg(xlang_dir().join("ts_peer.cjs")).args(args);
    run(cmd).await
}

async fn py(args: &[&str]) -> String {
    let mut cmd = Command::new("uv");
    cmd.args(["run", "--quiet", "--project"])
        .arg(python_sdk())
        .args(["--extra", "grpc", "python"])
        .arg(xlang_dir().join("py_peer.py"))
        .args(args);
    run(cmd).await
}

/// The JSON body Rust writes for `event`.
fn body(event: &AccountEvent) -> Value {
    serde_json::from_slice(&event.to_payload().unwrap()).unwrap()
}

async fn rust_write(store: Arc<dyn EventStorePort>, tenant: &str, id: &str, n: usize) {
    let repo = EventStoreRepository::<Account>::new(store, tenant);
    let mut instance = AggregateInstance::new(id.to_string(), Account::default());
    let mut events = fixture_events(id);
    events.truncate(n);
    instance.add_events(events).unwrap();
    repo.save(&mut instance).await.unwrap();
}

#[tokio::test]
#[ignore = "needs node and uv; run with `make test-xlang`"]
async fn typescript_writes_rust_reads() {
    let server = spawn_server().await;
    let store: Arc<dyn EventStorePort> = Arc::new(connect(&server.addr).await);
    let tenant = unique_tenant();
    ts(&["write", &server.addr, &tenant, "acct-ts"]).await;

    let loaded = EventStoreRepository::<Account>::new(store, &tenant)
        .load("acct-ts")
        .await
        .unwrap()
        .expect("stream written by TypeScript");
    assert_eq!(loaded.aggregate.events, fixture_events("acct-ts"));
}

#[tokio::test]
#[ignore = "needs node and uv; run with `make test-xlang`"]
async fn python_writes_rust_reads() {
    let server = spawn_server().await;
    let store: Arc<dyn EventStorePort> = Arc::new(connect(&server.addr).await);
    let tenant = unique_tenant();
    py(&["write", &server.addr, &tenant, "acct-py"]).await;

    let loaded = EventStoreRepository::<Account>::new(store, &tenant)
        .load("acct-py")
        .await
        .unwrap()
        .expect("stream written by Python");
    let mut expected = fixture_events("acct-py");
    expected.truncate(2);
    assert_eq!(loaded.aggregate.events, expected);
}

#[tokio::test]
#[ignore = "needs node and uv; run with `make test-xlang`"]
async fn rust_writes_typescript_reads() {
    let server = spawn_server().await;
    let store: Arc<dyn EventStorePort> = Arc::new(connect(&server.addr).await);
    let tenant = unique_tenant();
    rust_write(store, &tenant, "acct-rs", 3).await;

    let read: Value =
        serde_json::from_str(&ts(&["read", &server.addr, &tenant, "acct-rs"]).await).unwrap();
    let read = read.as_array().unwrap();
    let expected = fixture_events("acct-rs");
    assert_eq!(read.len(), expected.len());
    for (got, want) in read.iter().zip(&expected) {
        assert_eq!(got["event_type"], want.event_type());
        assert_eq!(got["event_version"], want.event_version());
        assert_eq!(got["aggregate_type"], "Account");
        assert_eq!(got["content_type"], "application/json");
        let mut data = got["data"].clone();
        // The TS event classes echo these fields (ADR-027, known deviation).
        data.as_object_mut().unwrap().remove("eventType");
        data.as_object_mut().unwrap().remove("schemaVersion");
        assert_eq!(data, body(want));
    }
}

#[tokio::test]
#[ignore = "needs node and uv; run with `make test-xlang`"]
async fn rust_writes_python_reads() {
    let server = spawn_server().await;
    let store: Arc<dyn EventStorePort> = Arc::new(connect(&server.addr).await);
    let tenant = unique_tenant();
    rust_write(store, &tenant, "acct-rs", 3).await;

    let read: Value =
        serde_json::from_str(&py(&["read", &server.addr, &tenant, "acct-rs"]).await).unwrap();
    let read = read.as_array().unwrap();
    let expected = fixture_events("acct-rs");
    assert_eq!(read.len(), expected.len());
    for (i, (got, want)) in read.iter().zip(&expected).enumerate() {
        assert_eq!(got["event_type"], want.event_type());
        assert_eq!(got["aggregate_type"], "Account");
        assert_eq!(got["aggregate_nonce"], json!(i + 1));
        let mut data = got["data"].clone();
        if got["class"] == "GenericDomainEvent" {
            // The Python fallback model carries the type as a field.
            assert_eq!(data["event_type"], want.event_type());
            data.as_object_mut().unwrap().remove("event_type");
        }
        assert_eq!(data, body(want));
    }
    // Registered Python classes validate Rust payloads (extra="forbid"):
    // proof that the body has exactly the schema's fields.
    assert_eq!(read[0]["class"], "AccountOpened");
    assert_eq!(read[1]["class"], "MoneyDeposited");
    // Not registered in the Python peer: generic fallback, data intact.
    assert_eq!(read[2]["class"], "GenericDomainEvent");
}
