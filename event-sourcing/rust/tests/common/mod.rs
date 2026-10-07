//! Live-server test harness: runs the real gRPC event store in-process.
//!
//! Backend: in-memory by default. Set `TEST_DATABASE_URL` to run the same
//! tests against the Postgres backend (each test uses its own tenant).

#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use event_sourcing_rust::client::{proto, EventDataStream, EventStoreClient, EventStorePort};
use event_sourcing_rust::error::{Error, Result};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::{sleep, Duration, Instant};

pub struct TestServer {
    pub addr: String,
    shutdown: Option<oneshot::Sender<()>>,
    handle: Option<JoinHandle<()>>,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

async fn backend() -> Arc<dyn eventstore_core::EventStore> {
    match std::env::var("TEST_DATABASE_URL") {
        Ok(url) if !url.is_empty() => eventstore_backend_postgres::PostgresStore::connect(&url)
            .await
            .expect("connect postgres backend"),
        _ => eventstore_backend_memory::InMemoryStore::new(),
    }
}

pub async fn spawn_server() -> TestServer {
    let port = portpicker::pick_unused_port().expect("free port");
    let addr = format!("127.0.0.1:{port}");
    let socket: SocketAddr = addr.parse().expect("socket addr");
    let service = eventstore_bin::Service {
        store: backend().await,
    };
    let (tx, rx) = oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        let _ = tonic::transport::Server::builder()
            .add_service(eventstore_bin::EventStoreServer::new(service))
            .serve_with_shutdown(socket, async move {
                let _ = rx.await;
            })
            .await;
    });
    TestServer {
        addr,
        shutdown: Some(tx),
        handle: Some(handle),
    }
}

pub async fn connect(addr: &str) -> EventStoreClient {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match EventStoreClient::connect(addr).await {
            Ok(client) => return client,
            Err(err) if Instant::now() < deadline => {
                let _ = err;
                sleep(Duration::from_millis(50)).await;
            }
            Err(err) => panic!("connect to test server: {err:?}"),
        }
    }
}

pub fn unique_tenant() -> String {
    format!("tenant-{}", uuid::Uuid::new_v4())
}

/// Fault-injecting port around a real client.
///
/// * `lose_acks`: the append reaches the store and commits, then the
///   acknowledgment is replaced by `Unavailable` (unknown outcome).
/// * `drop_requests`: the append fails with `Unavailable` before reaching
///   the store.
pub struct FaultyPort {
    pub inner: EventStoreClient,
    pub lose_acks: AtomicU32,
    pub drop_requests: AtomicU32,
    pub appends_sent: AtomicU32,
}

impl FaultyPort {
    pub fn new(inner: EventStoreClient) -> Self {
        Self {
            inner,
            lose_acks: AtomicU32::new(0),
            drop_requests: AtomicU32::new(0),
            appends_sent: AtomicU32::new(0),
        }
    }
}

/// Decrement `counter` if it is positive; true if it was decremented.
///
/// Explicit CAS loop rather than `fetch_update`, which newer toolchains
/// deprecate in favor of `try_update` (absent on older ones).
pub fn take(counter: &AtomicU32) -> bool {
    let mut current = counter.load(Ordering::SeqCst);
    while current > 0 {
        match counter.compare_exchange(current, current - 1, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => return true,
            Err(actual) => current = actual,
        }
    }
    false
}

#[async_trait]
impl EventStorePort for FaultyPort {
    async fn append(&self, req: proto::AppendRequest) -> Result<proto::AppendResponse> {
        if take(&self.drop_requests) {
            return Err(Error::from(tonic::Status::unavailable("request dropped")));
        }
        self.appends_sent.fetch_add(1, Ordering::SeqCst);
        let resp = self.inner.append(req).await?;
        if take(&self.lose_acks) {
            return Err(Error::from(tonic::Status::unavailable("ack lost")));
        }
        Ok(resp)
    }

    async fn read_stream(
        &self,
        req: proto::ReadStreamRequest,
    ) -> Result<proto::ReadStreamResponse> {
        self.inner.read_stream(req).await
    }

    async fn read_all(&self, req: proto::ReadAllRequest) -> Result<proto::ReadAllResponse> {
        self.inner.read_all(req).await
    }

    async fn subscribe(&self, req: proto::SubscribeRequest) -> Result<EventDataStream> {
        self.inner.subscribe(req).await
    }
}
