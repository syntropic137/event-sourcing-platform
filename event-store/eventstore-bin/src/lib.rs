use std::sync::Arc;

use eventstore_core::{proto, EventStore as EventStoreTrait};
use eventstore_proto::gen::event_store_server::EventStore;
use eventstore_proto::gen::{
    AppendRequest, GetServerInfoRequest, GetServerInfoResponse, ReadAllRequest, ReadStreamRequest,
};
use tokio_stream::{Stream, StreamExt};
use tonic::{Request, Response, Status};
use tracing::{error, info, instrument, warn};

pub use eventstore_proto::gen::event_store_server::EventStoreServer;
pub use eventstore_proto::gen::SubscribeResponse;

pub struct Service {
    pub store: Arc<dyn EventStoreTrait>,
}

/// Version of this server binary, reported by `GetServerInfo`.
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Build the `GetServerInfo` response for a backend. Capabilities come from
/// the backend so a server never advertises a guarantee its storage lacks.
pub fn server_info(store: &dyn EventStoreTrait) -> GetServerInfoResponse {
    GetServerInfoResponse {
        server_version: SERVER_VERSION.to_string(),
        api_version: eventstore_proto::API_VERSION.to_string(),
        backend: store.backend_kind().to_string(),
        capabilities: store
            .capabilities()
            .into_iter()
            .map(str::to_string)
            .collect(),
    }
}

#[tonic::async_trait]
impl EventStore for Service {
    #[instrument(name = "rpc.append", skip(self, request), fields(
        aggregate_id = %request.get_ref().aggregate_id,
        aggregate_type = %request.get_ref().aggregate_type,
    ))]
    async fn append(
        &self,
        request: Request<AppendRequest>,
    ) -> Result<Response<proto::AppendResponse>, Status> {
        let req = request.into_inner();
        match self.store.append(req).await {
            Ok(resp) => {
                info!(
                    last_global_nonce = resp.last_global_nonce,
                    last_aggregate_nonce = resp.last_aggregate_nonce,
                    "append ok"
                );
                Ok(Response::new(resp))
            }
            Err(e) => {
                warn!(error = %e, "append failed");
                Err(e.to_status())
            }
        }
    }

    #[instrument(name = "rpc.read_stream", skip(self, request), fields(
        aggregate_id = %request.get_ref().aggregate_id,
        from_aggregate_nonce = request.get_ref().from_aggregate_nonce,
        max_count = request.get_ref().max_count,
        forward = request.get_ref().forward,
    ))]
    async fn read_stream(
        &self,
        request: Request<ReadStreamRequest>,
    ) -> Result<Response<proto::ReadStreamResponse>, Status> {
        let req = request.into_inner();
        match self.store.read_stream(req).await {
            Ok(resp) => {
                info!(
                    events = resp.events.len(),
                    is_end = resp.is_end,
                    next_from_aggregate_nonce = resp.next_from_aggregate_nonce,
                    "read_stream ok"
                );
                Ok(Response::new(resp))
            }
            Err(e) => {
                warn!(error = %e, "read_stream failed");
                Err(e.to_status())
            }
        }
    }

    #[instrument(name = "rpc.read_all", skip(self, request), fields(
        from_global_nonce = request.get_ref().from_global_nonce,
        max_count = request.get_ref().max_count,
        forward = request.get_ref().forward,
    ))]
    async fn read_all(
        &self,
        request: Request<ReadAllRequest>,
    ) -> Result<Response<proto::ReadAllResponse>, Status> {
        let req = request.into_inner();
        match self.store.read_all(req).await {
            Ok(resp) => {
                info!(
                    events = resp.events.len(),
                    is_end = resp.is_end,
                    next_from_global_nonce = resp.next_from_global_nonce,
                    "read_all ok"
                );
                Ok(Response::new(resp))
            }
            Err(e) => {
                warn!(error = %e, "read_all failed");
                Err(e.to_status())
            }
        }
    }

    #[instrument(name = "rpc.get_server_info", skip(self, _request))]
    async fn get_server_info(
        &self,
        _request: Request<GetServerInfoRequest>,
    ) -> Result<Response<GetServerInfoResponse>, Status> {
        Ok(Response::new(server_info(self.store.as_ref())))
    }

    type SubscribeStream =
        Pin<Box<dyn Stream<Item = Result<SubscribeResponse, Status>> + Send + 'static>>;

    #[instrument(name = "rpc.subscribe", skip(self, request), fields(
        aggregate_id_prefix = %request.get_ref().aggregate_id_prefix,
        from_global_nonce = request.get_ref().from_global_nonce,
    ))]
    async fn subscribe(
        &self,
        request: Request<proto::SubscribeRequest>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        let req = request.into_inner();
        #[allow(clippy::result_large_err)] // tonic::Status is required by the gRPC trait
        let stream = self.store.subscribe(req).map(|res| {
            res.map_err(|e| {
                error!(error = %e, "subscribe stream error");
                e.to_status()
            })
        });
        Ok(Response::new(Box::pin(stream)))
    }
}

use std::pin::Pin;

pub async fn resolve_backend() -> anyhow::Result<Arc<dyn EventStoreTrait>> {
    let backend = std::env::var("BACKEND").unwrap_or_else(|_| "memory".to_string());
    match backend.as_str() {
        "memory" => Ok(eventstore_backend_memory::InMemoryStore::new()),
        "postgres" => {
            let url = std::env::var("DATABASE_URL")
                .map_err(|_| anyhow::anyhow!("DATABASE_URL must be set when BACKEND=postgres"))?;
            // Pool size and timeouts: PG_* env vars, see
            // docs/operations/POSTGRES-CONNECTIONS.md (#368, #370).
            let config = eventstore_backend_postgres::PostgresConfig::from_env()?;
            info!(?config, "postgres pool and timeout settings");
            let store =
                eventstore_backend_postgres::PostgresStore::connect_with_config(&url, &config)
                    .await?;
            Ok(store)
        }
        other => anyhow::bail!("unsupported BACKEND '{other}'. Supported: memory, postgres"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    fn set_env_and_get_prev<K: AsRef<str>, V: AsRef<str>>(
        key: K,
        val: Option<V>,
    ) -> Option<String> {
        let key = key.as_ref().to_string();
        let prev = std::env::var(&key).ok();
        match val {
            Some(v) => std::env::set_var(&key, v.as_ref()),
            None => std::env::remove_var(&key),
        }
        prev
    }

    #[test]
    fn server_info_reports_memory_backend_and_capabilities() {
        let store = eventstore_backend_memory::InMemoryStore::new();
        let info = server_info(store.as_ref());
        assert_eq!(info.server_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.api_version, "eventstore.v1");
        assert_eq!(info.backend, "memory");
        assert!(info
            .capabilities
            .iter()
            .any(|c| c == eventstore_core::capabilities::COMMIT_ORDERED_GLOBAL_NONCE));
    }

    /// A backend that does not override the server-info hooks.
    struct BareStore;

    #[tonic::async_trait]
    impl EventStoreTrait for BareStore {
        async fn append(
            &self,
            _req: proto::AppendRequest,
        ) -> Result<proto::AppendResponse, eventstore_core::StoreError> {
            unimplemented!()
        }
        async fn read_stream(
            &self,
            _req: proto::ReadStreamRequest,
        ) -> Result<proto::ReadStreamResponse, eventstore_core::StoreError> {
            unimplemented!()
        }
        async fn read_all(
            &self,
            _req: proto::ReadAllRequest,
        ) -> Result<proto::ReadAllResponse, eventstore_core::StoreError> {
            unimplemented!()
        }
        fn subscribe(
            &self,
            _req: proto::SubscribeRequest,
        ) -> eventstore_core::StoreStream<SubscribeResponse> {
            Box::pin(tokio_stream::empty())
        }
    }

    #[test]
    fn server_info_backend_defaults_advertise_nothing() {
        let info = server_info(&BareStore);
        assert_eq!(info.backend, "unknown");
        assert!(
            info.capabilities.is_empty(),
            "a backend must opt in to every capability"
        );
    }

    #[tokio::test]
    #[serial]
    async fn resolve_backend_defaults_to_memory() {
        let prev = set_env_and_get_prev("BACKEND", None::<&str>);
        let store = resolve_backend()
            .await
            .expect("memory backend should be supported");
        assert!(Arc::strong_count(&store) >= 1);
        match prev {
            Some(v) => std::env::set_var("BACKEND", v),
            None => std::env::remove_var("BACKEND"),
        }
    }

    #[tokio::test]
    #[serial]
    async fn resolve_backend_memory_explicit() {
        let prev = set_env_and_get_prev("BACKEND", Some("memory"));
        let store = resolve_backend()
            .await
            .expect("explicit memory should work");
        assert!(Arc::strong_count(&store) >= 1);
        match prev {
            Some(v) => std::env::set_var("BACKEND", v),
            None => std::env::remove_var("BACKEND"),
        }
    }

    #[tokio::test]
    #[serial]
    async fn resolve_backend_rejects_invalid_postgres_settings_before_connecting() {
        let prev_backend = set_env_and_get_prev("BACKEND", Some("postgres"));
        let prev_url = set_env_and_get_prev("DATABASE_URL", Some("postgres://u:p@127.0.0.1:1/db"));
        let prev_pool = set_env_and_get_prev("PG_POOL_MAX_CONNECTIONS", Some("lots"));
        let res = resolve_backend().await;
        for (k, v) in [
            ("BACKEND", prev_backend),
            ("DATABASE_URL", prev_url),
            ("PG_POOL_MAX_CONNECTIONS", prev_pool),
        ] {
            set_env_and_get_prev(k, v);
        }
        let msg = format!("{:#}", res.err().expect("invalid pool size must fail"));
        assert!(msg.contains("PG_POOL_MAX_CONNECTIONS"), "{msg}");
    }

    #[tokio::test]
    #[serial]
    async fn resolve_backend_unsupported_errors() {
        let prev = set_env_and_get_prev("BACKEND", Some("nope"));
        let res = resolve_backend().await;
        assert!(res.is_err(), "unsupported backend should error");
        let msg = format!("{:#}", res.err().unwrap());
        assert!(msg.contains("unsupported BACKEND"));
        match prev {
            Some(v) => std::env::set_var("BACKEND", v),
            None => std::env::remove_var("BACKEND"),
        }
    }
}
