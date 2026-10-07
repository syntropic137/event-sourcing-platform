//! Live cross-language round trips against a real event store (ADR-027), in
//! all six directions between TypeScript, Python and Rust. Uses the real SDKs
//! through `tests/xlang/ts_peer.cjs` and `tests/xlang/py_peer.py`; their
//! event classes are registered at every version and Python's are strict
//! (`extra="forbid"`), so a read proves the payload holds exactly the
//! schema's fields.
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

#[derive(Clone, Copy, Debug, PartialEq)]
enum Peer {
    Ts,
    Py,
}

async fn peer(p: Peer, args: &[&str]) -> String {
    let mut cmd = match p {
        Peer::Ts => {
            let mut c = Command::new("node");
            c.arg(xlang_dir().join("ts_peer.cjs"));
            c
        }
        Peer::Py => {
            let mut c = Command::new("uv");
            c.args(["run", "--quiet", "--project"])
                .arg(python_sdk())
                .args(["--extra", "grpc", "python"])
                .arg(xlang_dir().join("py_peer.py"));
            c
        }
    };
    cmd.args(args);
    run(cmd).await
}

async fn peer_write(p: Peer, addr: &str, tenant: &str, id: &str) {
    peer(p, &["write", addr, tenant, id]).await;
}

async fn peer_read(p: Peer, addr: &str, tenant: &str, id: &str) -> Vec<Value> {
    let out = peer(p, &["read", addr, tenant, id]).await;
    serde_json::from_str::<Value>(&out)
        .unwrap()
        .as_array()
        .unwrap()
        .clone()
}

/// The JSON body Rust writes for `event`.
fn body(event: &AccountEvent) -> Value {
    serde_json::from_slice(&event.to_payload().unwrap()).unwrap()
}

/// The class each peer decodes fixture event `i` into (all registered).
fn class(p: Peer, i: usize) -> &'static str {
    match (p, i) {
        (_, 0) => "AccountOpened",
        (_, 1) => "MoneyDeposited",
        (_, 2) => "AccountClosed",
        _ => unreachable!(),
    }
}

/// What a peer reports for stream `id` matches the fixture events exactly:
/// decoded class, type, version (SDK-reported and stored), and payload.
fn assert_peer_read(p: Peer, read: &[Value], id: &str) {
    let expected = fixture_events(id);
    assert_eq!(read.len(), expected.len(), "{p:?}");
    for (i, (got, want)) in read.iter().zip(&expected).enumerate() {
        let ctx = format!("{p:?} event {i}: {got}");
        assert_eq!(got["class"], class(p, i), "{ctx}");
        assert_eq!(got["event_type"], want.event_type(), "{ctx}");
        assert_eq!(got["event_version"], want.event_version(), "{ctx}");
        // The SDK exposes the stored version, and it is the wire value.
        assert_eq!(got["stored_event_type"], want.event_type(), "{ctx}");
        assert_eq!(got["stored_event_version"], want.event_version(), "{ctx}");
        assert_eq!(got["wire_event_type"], want.event_type(), "{ctx}");
        assert_eq!(got["wire_event_version"], want.event_version(), "{ctx}");
        assert_eq!(got["aggregate_type"], "Account", "{ctx}");
        assert_eq!(got["aggregate_nonce"], json!(i + 1), "{ctx}");
        assert_eq!(got["content_type"], "application/json", "{ctx}");
        assert_eq!(got["data"], body(want), "{ctx}");
    }
}

async fn rust_read(store: Arc<dyn EventStorePort>, tenant: &str, id: &str) -> Vec<AccountEvent> {
    EventStoreRepository::<Account>::new(store, tenant)
        .load(id)
        .await
        .unwrap()
        .expect("stream exists")
        .aggregate
        .events
}

async fn rust_write(store: Arc<dyn EventStorePort>, tenant: &str, id: &str) {
    let repo = EventStoreRepository::<Account>::new(store, tenant);
    let mut instance = AggregateInstance::new(id.to_string(), Account::default());
    instance.add_events(fixture_events(id)).unwrap();
    repo.save(&mut instance).await.unwrap();
}

async fn writes_rust_reads(p: Peer) {
    let server = spawn_server().await;
    let store: Arc<dyn EventStorePort> = Arc::new(connect(&server.addr).await);
    let tenant = unique_tenant();
    peer_write(p, &server.addr, &tenant, "acct-x").await;
    assert_eq!(
        rust_read(store, &tenant, "acct-x").await,
        fixture_events("acct-x")
    );
}

async fn rust_writes_reads(p: Peer) {
    let server = spawn_server().await;
    let store: Arc<dyn EventStorePort> = Arc::new(connect(&server.addr).await);
    let tenant = unique_tenant();
    rust_write(store, &tenant, "acct-rs").await;
    let read = peer_read(p, &server.addr, &tenant, "acct-rs").await;
    assert_peer_read(p, &read, "acct-rs");
}

async fn writes_reads(writer: Peer, reader: Peer) {
    let server = spawn_server().await;
    let tenant = unique_tenant();
    peer_write(writer, &server.addr, &tenant, "acct-x").await;
    let read = peer_read(reader, &server.addr, &tenant, "acct-x").await;
    assert_peer_read(reader, &read, "acct-x");
}

#[tokio::test]
#[ignore = "needs node and uv; run with `make test-xlang`"]
async fn typescript_writes_rust_reads() {
    writes_rust_reads(Peer::Ts).await;
}

#[tokio::test]
#[ignore = "needs node and uv; run with `make test-xlang`"]
async fn python_writes_rust_reads() {
    writes_rust_reads(Peer::Py).await;
}

#[tokio::test]
#[ignore = "needs node and uv; run with `make test-xlang`"]
async fn rust_writes_typescript_reads() {
    rust_writes_reads(Peer::Ts).await;
}

#[tokio::test]
#[ignore = "needs node and uv; run with `make test-xlang`"]
async fn rust_writes_python_reads() {
    rust_writes_reads(Peer::Py).await;
}

#[tokio::test]
#[ignore = "needs node and uv; run with `make test-xlang`"]
async fn typescript_writes_python_reads() {
    writes_reads(Peer::Ts, Peer::Py).await;
}

#[tokio::test]
#[ignore = "needs node and uv; run with `make test-xlang`"]
async fn python_writes_typescript_reads() {
    writes_reads(Peer::Py, Peer::Ts).await;
}

/// Each peer reads its own stream back (the baseline the other directions
/// are compared with).
#[tokio::test]
#[ignore = "needs node and uv; run with `make test-xlang`"]
async fn each_sdk_reads_its_own_stream() {
    for p in [Peer::Ts, Peer::Py] {
        writes_reads(p, p).await;
    }
}
