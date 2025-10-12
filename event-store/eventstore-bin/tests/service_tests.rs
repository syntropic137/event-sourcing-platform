use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use eventstore_core::EventStore as EventStoreTrait;
use eventstore_proto::gen::event_store_client::EventStoreClient;
use eventstore_proto::gen::event_store_server::EventStore;
use eventstore_proto::gen::{
    self as proto, AppendRequest, EventData, EventMetadata, ReadStreamRequest, SubscribeRequest,
};
use tokio::task::JoinHandle;
use tokio_stream::{Stream, StreamExt};
use tonic::transport::Server;
use tonic::{Request, Response, Status};

const TENANT: &str = "tenant-service";

struct Service {
    store: Arc<dyn EventStoreTrait>,
}

// Helper to read next non-empty subscribe message within a timeout
#[allow(dead_code)]
async fn next_event_within(
    stream: &mut tonic::Streaming<proto::SubscribeResponse>,
    dur: Duration,
) -> proto::EventData {
    loop {
        let msg = tokio::time::timeout(dur, stream.message())
            .await
            .expect("timeout waiting for subscribe message")
            .expect("stream closed")
            .expect("stream error");
        if let Some(ev) = msg.event {
            return ev;
        }
        // else: empty heartbeat/idle; loop again
    }
}

#[tokio::test]
#[serial_test::serial]
async fn service_append_and_read_with_postgres_backend() {
    // Start Postgres via testcontainers
    use testcontainers::runners::AsyncRunner;
    use testcontainers_modules::postgres::Postgres as PgImage;

    let container = PgImage::default().start().await.expect("start postgres");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("get mapped port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    // Connect store and run migrations
    let store = eventstore_backend_postgres::PostgresStore::connect(&url)
        .await
        .expect("connect+init");

    // Spawn gRPC server with Postgres backend
    let (endpoint, _jh) = spawn_server_with_store(store).await;
    let mut client = EventStoreClient::connect(endpoint).await.unwrap();

    // Append and read
    let req = AppendRequest {
        tenant_id: TENANT.into(),
        aggregate_id: "Order-100".to_string(),
        aggregate_type: "Order".to_string(),
        expected_aggregate_nonce: 0,
        idempotency_key: String::new(),
        events: vec![
            make_event("Order-100", "Order", 1, b"pg1"),
            make_event("Order-100", "Order", 2, b"pg2"),
        ],
    };
    let resp = client.append(req).await.unwrap().into_inner();
    assert_eq!(resp.last_aggregate_nonce, 2);

    let read = ReadStreamRequest {
        tenant_id: TENANT.into(),
        aggregate_id: "Order-100".into(),
        from_aggregate_nonce: 1,
        max_count: 10,
        forward: true,
    };
    let out = client.read_stream(read).await.unwrap().into_inner();
    assert_eq!(out.events.len(), 2);
    assert_eq!(out.events[0].payload, b"pg1");
    assert_eq!(out.events[1].payload, b"pg2");
}

#[tonic::async_trait]
impl EventStore for Service {
    async fn append(
        &self,
        request: Request<AppendRequest>,
    ) -> Result<Response<proto::AppendResponse>, Status> {
        let req = request.into_inner();
        self.store
            .append(req)
            .await
            .map(Response::new)
            .map_err(|e| e.to_status())
    }

    async fn read_stream(
        &self,
        request: Request<ReadStreamRequest>,
    ) -> Result<Response<proto::ReadStreamResponse>, Status> {
        let req = request.into_inner();
        self.store
            .read_stream(req)
            .await
            .map(Response::new)
            .map_err(|e| e.to_status())
    }

    type SubscribeStream =
        Pin<Box<dyn Stream<Item = Result<proto::SubscribeResponse, Status>> + Send + 'static>>;

    async fn subscribe(
        &self,
        request: Request<SubscribeRequest>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        let req = request.into_inner();
        let stream = self
            .store
            .subscribe(req)
            .map(|res| res.map_err(|e| e.to_status()));
        Ok(Response::new(Box::pin(stream)))
    }
}

async fn spawn_server_with_store(
    store: Arc<dyn EventStoreTrait>,
) -> (String, JoinHandle<anyhow::Result<()>>) {
    // Bind to ephemeral port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();

    // Create service with provided backend
    let svc = Service { store };
    let router = eventstore_proto::gen::event_store_server::EventStoreServer::new(svc);

    let handle = tokio::spawn(async move {
        Server::builder()
            .add_service(router)
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .map_err(|e| anyhow::anyhow!(e))
    });

    (format!("http://{addr}"), handle)
}

async fn spawn_server() -> (String, JoinHandle<anyhow::Result<()>>) {
    // default: in-memory
    let store = eventstore_backend_memory::InMemoryStore::new();
    spawn_server_with_store(store).await
}

fn make_event(
    aggregate_id: &str,
    aggregate_type: &str,
    aggregate_nonce: u64,
    payload: &[u8],
) -> EventData {
    EventData {
        meta: Some(EventMetadata {
            event_id: uuid::Uuid::new_v4().to_string(),
            aggregate_id: aggregate_id.to_string(),
            aggregate_type: aggregate_type.to_string(),
            aggregate_nonce,
            event_type: "Test".to_string(),
            event_version: 1,
            content_type: "application/octet-stream".to_string(),
            tenant_id: TENANT.to_string(),
            ..Default::default()
        }),
        payload: payload.to_vec(),
    }
}

#[tokio::test]
async fn service_append_and_read_stream_forward() {
    let (endpoint, _jh) = spawn_server().await;

    let mut client = EventStoreClient::connect(endpoint).await.unwrap();

    // Append two events
    let req = AppendRequest {
        tenant_id: TENANT.into(),
        aggregate_id: "Order-1".to_string(),
        aggregate_type: "Order".to_string(),
        expected_aggregate_nonce: 0,
        idempotency_key: String::new(),
        events: vec![
            make_event("Order-1", "Order", 1, b"a"),
            make_event("Order-1", "Order", 2, b"b"),
        ],
    };
    let resp = client.append(req).await.unwrap().into_inner();
    assert_eq!(resp.last_aggregate_nonce, 2);

    // Read forward from 1
    let read = ReadStreamRequest {
        tenant_id: TENANT.into(),
        aggregate_id: "Order-1".into(),
        from_aggregate_nonce: 1,
        max_count: 10,
        forward: true,
    };
    let out = client.read_stream(read).await.unwrap().into_inner();
    assert_eq!(out.events.len(), 2);
    assert!(out.is_end);
    assert_eq!(out.events[0].payload, b"a");
    assert_eq!(out.events[1].payload, b"b");
}

#[tokio::test]
async fn service_subscribe_replay_and_live() {
    let (endpoint, _jh) = spawn_server().await;
    let mut client = EventStoreClient::connect(endpoint.clone()).await.unwrap();

    // Seed some events
    let req = AppendRequest {
        tenant_id: TENANT.into(),
        aggregate_id: "Order-2".to_string(),
        aggregate_type: "Order".to_string(),
        expected_aggregate_nonce: 0,
        idempotency_key: String::new(),
        events: vec![
            make_event("Order-2", "Order", 1, b"x"),
            make_event("Order-2", "Order", 2, b"y"),
        ],
    };
    client.append(req).await.unwrap();

    // Subscribe from start and then append one more live event
    let mut sub = EventStoreClient::connect(endpoint.clone()).await.unwrap();
    let request = SubscribeRequest {
        tenant_id: TENANT.into(),
        aggregate_id_prefix: "Order-".into(),
        from_global_nonce: 0,
    };
    let mut stream = sub.subscribe(request).await.unwrap().into_inner();

    // Collect replay first event
    let first = tokio::time::timeout(Duration::from_secs(2), stream.message())
        .await
        .expect("timeout waiting for replay")
        .unwrap()
        .unwrap();
    assert_eq!(first.event.unwrap().payload, b"x");

    // Append a live event and expect it to appear
    let live_append = AppendRequest {
        tenant_id: TENANT.into(),
        aggregate_id: "Order-2".to_string(),
        aggregate_type: "Order".to_string(),
        expected_aggregate_nonce: 2,
        idempotency_key: String::new(),
        events: vec![make_event("Order-2", "Order", 3, b"z")],
    };
    client.append(live_append).await.unwrap();

    let live = tokio::time::timeout(Duration::from_secs(2), stream.message())
        .await
        .expect("timeout waiting for live")
        .unwrap()
        .unwrap();
    assert_eq!(live.event.unwrap().payload, b"y"); // second replay

    let live2 = tokio::time::timeout(Duration::from_secs(2), stream.message())
        .await
        .expect("timeout waiting for next")
        .unwrap()
        .unwrap();
    assert_eq!(live2.event.unwrap().payload, b"z"); // live event
}

#[tokio::test]
async fn service_append_concurrency_conflict_exact() {
    let (endpoint, _jh) = spawn_server().await;

    let mut client = EventStoreClient::connect(endpoint.clone()).await.unwrap();

    // First append creates the stream (ANY)
    let first = AppendRequest {
        tenant_id: TENANT.into(),
        aggregate_id: "Order-3".to_string(),
        aggregate_type: "Order".to_string(),
        expected_aggregate_nonce: 0,
        idempotency_key: String::new(),
        events: vec![make_event("Order-3", "Order", 1, b"1")],
    };
    let resp = client.append(first).await.unwrap().into_inner();
    assert_eq!(resp.last_aggregate_nonce, 1);

    // Exact(1) should succeed (appending second event)
    let ok_again = AppendRequest {
        tenant_id: TENANT.into(),
        aggregate_id: "Order-3".to_string(),
        aggregate_type: "Order".to_string(),
        expected_aggregate_nonce: 1,
        idempotency_key: String::new(),
        events: vec![make_event("Order-3", "Order", 2, b"2")],
    };
    let resp2 = client.append(ok_again).await.unwrap().into_inner();
    assert_eq!(resp2.last_aggregate_nonce, 2);

    // Reusing Exact(1) should now fail with Aborted (concurrency)
    let should_conflict = AppendRequest {
        tenant_id: TENANT.into(),
        aggregate_id: "Order-3".to_string(),
        aggregate_type: "Order".to_string(),
        expected_aggregate_nonce: 1,
        idempotency_key: String::new(),
        events: vec![make_event("Order-3", "Order", 3, b"3")],
    };
    let err = client
        .append(should_conflict)
        .await
        .expect_err("expected concurrency error");
    assert_eq!(err.code(), tonic::Code::Aborted);
}

#[tokio::test]
async fn service_pg_concurrency_conflict_exact() {
    // Start Postgres via testcontainers (simplified, matching working test)
    use testcontainers::runners::AsyncRunner;
    use testcontainers_modules::postgres::Postgres as PgImage;

    let container = PgImage::default().start().await.expect("start postgres");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("get mapped port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    // Connect store and run migrations
    let store = eventstore_backend_postgres::PostgresStore::connect(&url)
        .await
        .expect("connect+init");

    // Spawn gRPC server with Postgres backend
    let (endpoint, _jh) = spawn_server_with_store(store).await;

    let mut client = EventStoreClient::connect(endpoint.clone()).await.unwrap();

    // First append creates the stream (ANY)
    let first = AppendRequest {
        tenant_id: TENANT.into(),
        aggregate_id: "Order-77".to_string(),
        aggregate_type: "Order".to_string(),
        expected_aggregate_nonce: 0,
        idempotency_key: String::new(),
        events: vec![make_event("Order-77", "Order", 1, b"1")],
    };
    let resp = client.append(first).await.unwrap().into_inner();
    assert_eq!(resp.last_aggregate_nonce, 1);

    // Exact(1) should succeed (appending second event)
    let ok_again = AppendRequest {
        tenant_id: TENANT.into(),
        aggregate_id: "Order-77".to_string(),
        aggregate_type: "Order".to_string(),
        expected_aggregate_nonce: 1,
        idempotency_key: String::new(),
        events: vec![make_event("Order-77", "Order", 2, b"2")],
    };
    let resp2 = client.append(ok_again).await.unwrap().into_inner();
    assert_eq!(resp2.last_aggregate_nonce, 2);

    // Reusing Exact(1) should now fail with Aborted (concurrency)
    let should_conflict = AppendRequest {
        tenant_id: TENANT.into(),
        aggregate_id: "Order-77".to_string(),
        aggregate_type: "Order".to_string(),
        expected_aggregate_nonce: 1,
        idempotency_key: String::new(),
        events: vec![make_event("Order-77", "Order", 3, b"3")],
    };
    let err = client
        .append(should_conflict)
        .await
        .expect_err("expected concurrency error");
    assert_eq!(err.code(), tonic::Code::Aborted);
}

#[tokio::test]
async fn service_subscribe_filters_by_stream_prefix() {
    let (endpoint, _jh) = spawn_server().await;
    let mut client = EventStoreClient::connect(endpoint.clone()).await.unwrap();

    // Seed two categories
    let seed_order = AppendRequest {
        tenant_id: TENANT.into(),
        aggregate_id: "Order-9".to_string(),
        aggregate_type: "Order".to_string(),
        expected_aggregate_nonce: 0,
        idempotency_key: String::new(),
        events: vec![
            make_event("Order-9", "Order", 1, b"o1"),
            make_event("Order-9", "Order", 2, b"o2"),
        ],
    };
    client.append(seed_order).await.unwrap();

    let seed_payment = AppendRequest {
        tenant_id: TENANT.into(),
        aggregate_id: "Payment-1".to_string(),
        aggregate_type: "Payment".to_string(),
        expected_aggregate_nonce: 0,
        idempotency_key: String::new(),
        events: vec![make_event("Payment-1", "Payment", 1, b"p1")],
    };
    client.append(seed_payment).await.unwrap();

    // Subscribe only to Order-*
    let mut sub = EventStoreClient::connect(endpoint.clone()).await.unwrap();
    let request = SubscribeRequest {
        tenant_id: TENANT.into(),
        aggregate_id_prefix: "Order-".into(),
        from_global_nonce: 0,
    };
    let mut stream = sub.subscribe(request).await.unwrap().into_inner();

    // Expect two replay events from Order-9, not Payment-1
    let r1 = tokio::time::timeout(Duration::from_secs(2), stream.message())
        .await
        .expect("timeout waiting for replay 1")
        .unwrap()
        .unwrap();
    assert_eq!(r1.event.as_ref().unwrap().payload, b"o1");

    let r2 = tokio::time::timeout(Duration::from_secs(2), stream.message())
        .await
        .expect("timeout waiting for replay 2")
        .unwrap()
        .unwrap();
    assert_eq!(r2.event.as_ref().unwrap().payload, b"o2");

    // Append live to Payment (should NOT arrive) and Order (should arrive)
    let _ = client
        .append(AppendRequest {
            tenant_id: TENANT.into(),
            aggregate_id: "Payment-1".into(),
            aggregate_type: "Payment".into(),
            expected_aggregate_nonce: 1,
            idempotency_key: String::new(),
            events: vec![make_event("Payment-1", "Payment", 2, b"p2")],
        })
        .await
        .unwrap();

    let _ = client
        .append(AppendRequest {
            tenant_id: TENANT.into(),
            aggregate_id: "Order-9".into(),
            aggregate_type: "Order".into(),
            expected_aggregate_nonce: 2,
            idempotency_key: String::new(),
            events: vec![make_event("Order-9", "Order", 3, b"o3")],
        })
        .await
        .unwrap();

    let live = tokio::time::timeout(Duration::from_secs(3), stream.message())
        .await
        .expect("timeout waiting for live order")
        .unwrap()
        .unwrap();
    assert_eq!(live.event.as_ref().unwrap().payload, b"o3");
}

#[tokio::test]
async fn service_read_stream_backward_slice() {
    let (endpoint, _jh) = spawn_server().await;
    let mut client = EventStoreClient::connect(endpoint).await.unwrap();

    // Append three events
    let req = AppendRequest {
        tenant_id: TENANT.into(),
        aggregate_id: "Order-4".to_string(),
        aggregate_type: "Order".to_string(),
        expected_aggregate_nonce: 0,
        idempotency_key: String::new(),
        events: vec![
            make_event("Order-4", "Order", 1, b"a"),
            make_event("Order-4", "Order", 2, b"b"),
            make_event("Order-4", "Order", 3, b"c"),
        ],
    };
    client.append(req).await.unwrap();

    // Read last two backward starting from version 3
    let read = ReadStreamRequest {
        tenant_id: TENANT.into(),
        aggregate_id: "Order-4".into(),
        from_aggregate_nonce: 3,
        max_count: 2,
        forward: false,
    };
    let out = client.read_stream(read).await.unwrap().into_inner();
    assert_eq!(out.events.len(), 2);
    assert_eq!(out.events[0].payload, b"c");
    assert_eq!(out.events[1].payload, b"b");
}
