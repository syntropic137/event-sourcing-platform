use anyhow::Result;
use eventstore_proto::gen::event_store_client::EventStoreClient;
use eventstore_proto::gen::{AppendRequest, ReadStreamRequest, SubscribeRequest};
use tonic::transport::Channel;

pub struct EventStore {
    inner: EventStoreClient<Channel>,
}

impl EventStore {
    pub async fn connect(addr: &str) -> Result<Self> {
        let inner = EventStoreClient::connect(format!("http://{addr}")).await?;
        Ok(Self { inner })
    }

    pub async fn append(
        &mut self,
        req: AppendRequest,
    ) -> Result<eventstore_proto::gen::AppendResponse> {
        let resp = self.inner.append(req).await?.into_inner();
        Ok(resp)
    }

    pub async fn read_stream(
        &mut self,
        req: ReadStreamRequest,
    ) -> Result<eventstore_proto::gen::ReadStreamResponse> {
        let resp = self.inner.read_stream(req).await?.into_inner();
        Ok(resp)
    }

    pub async fn subscribe(
        &mut self,
        req: SubscribeRequest,
    ) -> Result<tonic::Streaming<eventstore_proto::gen::SubscribeResponse>> {
        let stream = self.inner.subscribe(req).await?.into_inner();
        Ok(stream)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eventstore_proto::gen::{
        AppendRequest, EventData, EventMetadata, ReadStreamRequest, SubscribeRequest,
    };
    use std::net::SocketAddr;
    use tokio::sync::oneshot;
    use tokio::time::{sleep, Duration, Instant};
    use tonic::transport::Server;
    use tower_http::trace::TraceLayer;

    async fn spawn_memory_server(addr: &str) -> (oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
        let socket: SocketAddr = addr.parse().expect("valid socket address");
        let store = eventstore_backend_memory::InMemoryStore::new();
        let service = eventstore_bin::Service { store };
        let (tx, rx) = oneshot::channel::<()>();

        let handle = tokio::spawn(async move {
            let shutdown = async move {
                let _ = rx.await;
            };

            let result = Server::builder()
                .layer(TraceLayer::new_for_grpc())
                .add_service(eventstore_bin::EventStoreServer::new(service))
                .serve_with_shutdown(socket, shutdown)
                .await;

            if let Err(error) = result {
                eprintln!("event store server error: {error}");
            }
        });

        (tx, handle)
    }

    async fn connect_with_retry(addr: &str) -> EventStore {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match EventStore::connect(addr).await {
                Ok(client) => return client,
                Err(err) => {
                    if Instant::now() >= deadline {
                        panic!("connect to memory server: {err:?}");
                    }
                    sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

    fn sample_append_request() -> AppendRequest {
        AppendRequest {
            tenant_id: "tenant-a".into(),
            aggregate_id: "agg-1".into(),
            aggregate_type: "Order".into(),
            expected_aggregate_nonce: 0,
            idempotency_key: String::new(),
            events: vec![EventData {
                meta: Some(EventMetadata {
                    event_id: "evt-1".into(),
                    aggregate_id: "agg-1".into(),
                    aggregate_type: "Order".into(),
                    aggregate_nonce: 1,
                    event_type: "OrderCreated".into(),
                    event_version: 1,
                    content_type: "application/json".into(),
                    content_schema: String::new(),
                    correlation_id: String::new(),
                    causation_id: String::new(),
                    actor_id: String::new(),
                    tenant_id: "tenant-a".into(),
                    timestamp_unix_ms: 0,
                    recorded_time_unix_ms: 0,
                    payload_sha256: Vec::new(),
                    headers: Default::default(),
                    global_nonce: 0,
                }),
                payload: Vec::new(),
            }],
        }
    }

    fn sample_read_request() -> ReadStreamRequest {
        ReadStreamRequest {
            tenant_id: "tenant-a".into(),
            aggregate_id: "agg-1".into(),
            from_aggregate_nonce: 1,
            max_count: 10,
            forward: true,
        }
    }

    fn sample_subscribe_request() -> SubscribeRequest {
        SubscribeRequest {
            tenant_id: "tenant-a".into(),
            aggregate_id_prefix: String::new(),
            from_global_nonce: 1,
        }
    }

    #[tokio::test]
    async fn append_and_read_roundtrip() {
        let port = portpicker::pick_unused_port().expect("No ports free");
        let addr = format!("127.0.0.1:{port}");
        let (shutdown, handle) = spawn_memory_server(&addr).await;

        let mut store = connect_with_retry(&addr).await;

        let append = store
            .append(sample_append_request())
            .await
            .expect("append succeeds");
        assert_eq!(append.last_aggregate_nonce, 1);

        let read = store
            .read_stream(sample_read_request())
            .await
            .expect("read succeeds");
        assert_eq!(read.events.len(), 1);

        let _ = shutdown.send(());
        let _ = handle.await;
    }

    #[tokio::test]
    async fn subscribe_delivers_events() {
        let port = portpicker::pick_unused_port().expect("No ports free");
        let addr = format!("127.0.0.1:{port}");
        let (shutdown, handle) = spawn_memory_server(&addr).await;

        let mut writer = connect_with_retry(&addr).await;
        writer
            .append(sample_append_request())
            .await
            .expect("append succeeds");

        let mut reader = connect_with_retry(&addr).await;
        let mut stream = reader
            .subscribe(sample_subscribe_request())
            .await
            .expect("subscribe succeeds");

        let message = tokio::time::timeout(tokio::time::Duration::from_secs(5), stream.message())
            .await
            .expect("timeout waiting for event")
            .expect("stream response")
            .expect("event payload");

        assert_eq!(message.event.unwrap().meta.unwrap().aggregate_id, "agg-1");

        // Drop all client connections and streams before shutting down the server.
        // This prevents a deadlock where the server waits for connections to close,
        // and the test waits for the server to shut down.
        drop(stream);
        drop(reader);
        drop(writer);

        let _ = shutdown.send(());
        let _ = handle.await;
    }

    #[tokio::test]
    async fn connect_invalid_endpoint_fails() {
        let result = EventStore::connect("127.0.0.1:59999").await;
        assert!(result.is_err());
    }
}
