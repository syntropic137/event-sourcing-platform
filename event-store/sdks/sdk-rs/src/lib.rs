//! Low-level Rust client for the event store gRPC API.
//!
//! Connect with [`EventStore::connect`] (`host:port`, `http://...`, or
//! `https://...`) or, for TLS, credentials, timeouts and keepalive, with
//! [`ClientConfig`].

use std::future::Future;
use std::time::Duration;

use anyhow::Result;
use eventstore_proto::gen::event_store_client::EventStoreClient;
use eventstore_proto::gen::{AppendRequest, ReadAllRequest, ReadStreamRequest, SubscribeRequest};
use tonic::service::interceptor::InterceptedService;
use tonic::transport::Channel;
use tonic::{Request, Response, Status};

mod auth;
mod config;
mod server_info;
use auth::AuthInterceptor;
pub use auth::{Credentials, InvalidCredentials, SharedToken, TokenProvider};
pub use config::{
    ClientConfig, ConfigError, TlsConfig, DEFAULT_CONNECT_TIMEOUT,
    DEFAULT_HTTP2_KEEPALIVE_INTERVAL, DEFAULT_HTTP2_KEEPALIVE_TIMEOUT, DEFAULT_REQUEST_TIMEOUT,
    DEFAULT_TCP_KEEPALIVE,
};
pub use server_info::{capabilities, CompatibilityError, ServerInfo, SERVER_INFO_MIN_VERSION};

type Inner = EventStoreClient<InterceptedService<Channel, AuthInterceptor>>;

/// Cloning is cheap: clones share the underlying gRPC channel.
#[derive(Clone)]
pub struct EventStore {
    inner: Inner,
    request_timeout: Option<Duration>,
}

impl std::fmt::Debug for EventStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventStore")
            .field("request_timeout", &self.request_timeout)
            .finish_non_exhaustive()
    }
}

impl EventStore {
    /// Connect with default settings. `addr` is `host:port` (plaintext),
    /// `http://host:port`, or `https://host:port` (TLS, OS trust store).
    pub async fn connect(addr: &str) -> Result<Self> {
        Self::connect_with(ClientConfig::new(addr)).await
    }

    /// Connect with explicit settings. Same as [`ClientConfig::connect`].
    pub async fn connect_with(config: ClientConfig) -> Result<Self> {
        let endpoint = config.endpoint()?;
        let auth = AuthInterceptor::new(config.credentials_value())?;
        let channel = if config.is_lazy() {
            endpoint.connect_lazy()
        } else {
            match config.connect_timeout_value() {
                // tonic's connect timeout covers TCP and TLS; this outer bound
                // also covers the HTTP/2 handshake.
                Some(t) => tokio::time::timeout(t, endpoint.connect())
                    .await
                    .map_err(|_| Status::unavailable(format!("connect timed out after {t:?}")))??,
                None => endpoint.connect().await?,
            }
        };
        Ok(Self {
            inner: EventStoreClient::with_interceptor(channel, auth),
            request_timeout: config.request_timeout_value(),
        })
    }

    /// Wrap a unary message, attaching the request deadline as
    /// `grpc-timeout` so the server can give up too.
    fn unary<T>(&self, msg: T) -> Request<T> {
        let mut req = Request::new(msg);
        if let Some(t) = self.request_timeout {
            req.set_timeout(t);
        }
        req
    }

    pub async fn append(
        &mut self,
        req: AppendRequest,
    ) -> Result<eventstore_proto::gen::AppendResponse> {
        let req = self.unary(req);
        Ok(bounded(self.request_timeout, self.inner.append(req)).await?)
    }

    pub async fn read_stream(
        &mut self,
        req: ReadStreamRequest,
    ) -> Result<eventstore_proto::gen::ReadStreamResponse> {
        let req = self.unary(req);
        Ok(bounded(self.request_timeout, self.inner.read_stream(req)).await?)
    }

    /// Open a subscription. The request timeout bounds only opening the
    /// stream (until response headers arrive); the stream itself has no
    /// deadline (no `grpc-timeout` is sent) and ends with `UNAVAILABLE` when
    /// HTTP/2 keepalive finds the connection dead.
    pub async fn subscribe(
        &mut self,
        req: SubscribeRequest,
    ) -> Result<tonic::Streaming<eventstore_proto::gen::SubscribeResponse>> {
        let stream = bounded(
            self.request_timeout,
            self.inner.subscribe(Request::new(req)),
        )
        .await?;
        Ok(stream)
    }

    /// Read all events from a global position (for projections/catch-up).
    pub async fn read_all(
        &mut self,
        req: ReadAllRequest,
    ) -> Result<eventstore_proto::gen::ReadAllResponse> {
        let req = self.unary(req);
        Ok(bounded(self.request_timeout, self.inner.read_all(req)).await?)
    }
}

impl ClientConfig {
    /// Connect using this configuration.
    pub async fn connect(self) -> Result<EventStore> {
        EventStore::connect_with(self).await
    }
}

/// Await a call's response within `timeout`, mapping expiry to
/// `DEADLINE_EXCEEDED`. For streaming calls the response is the headers, so
/// this never bounds the stream body.
async fn bounded<R>(
    timeout: Option<Duration>,
    call: impl Future<Output = std::result::Result<Response<R>, Status>>,
) -> std::result::Result<R, Status> {
    let expired = |t: Duration| {
        Status::deadline_exceeded(format!("event store request timed out after {t:?}"))
    };
    let resp = match timeout {
        Some(t) => match tokio::time::timeout(t, call).await {
            Ok(Ok(resp)) => resp,
            // tonic's own `grpc-timeout` enforcement can win the race; it
            // reports CANCELLED, which callers must not mistake for a
            // caller-side cancel.
            Ok(Err(status)) if is_tonic_timeout(&status) => return Err(expired(t)),
            Ok(Err(status)) => return Err(status),
            Err(_) => return Err(expired(t)),
        },
        None => call.await?,
    };
    Ok(resp.into_inner())
}

fn is_tonic_timeout(status: &Status) -> bool {
    let mut source = std::error::Error::source(status);
    while let Some(err) = source {
        if err.is::<tonic::TimeoutExpired>() {
            return true;
        }
        source = err.source();
    }
    false
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
