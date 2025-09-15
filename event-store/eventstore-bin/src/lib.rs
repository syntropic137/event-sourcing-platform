use std::sync::Arc;

use eventstore_core::{proto, EventStore as EventStoreTrait};
use eventstore_proto::gen::event_store_server::EventStore;
use eventstore_proto::gen::{AppendRequest, ReadStreamRequest};
use tokio_stream::{Stream, StreamExt};
use tonic::{Request, Response, Status};
use tracing::{error, info, instrument, warn};

pub use eventstore_proto::gen::event_store_server::EventStoreServer;
pub use eventstore_proto::gen::SubscribeResponse;

pub struct Service {
    pub store: Arc<dyn EventStoreTrait>,
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
                    next_aggregate_nonce = resp.next_aggregate_nonce,
                    last_global_nonce = resp.last_global_nonce,
                    next_global_nonce = resp.last_global_nonce + 1,
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

    type SubscribeStream =
        Pin<Box<dyn Stream<Item = Result<SubscribeResponse, Status>> + Send + 'static>>;

    #[instrument(name = "rpc.subscribe", skip(self, request), fields(
        aggregate_prefix = %request.get_ref().aggregate_prefix,
        from_global_nonce = request.get_ref().from_global_nonce,
    ))]
    async fn subscribe(
        &self,
        request: Request<proto::SubscribeRequest>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        let req = request.into_inner();
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
            let store = eventstore_backend_postgres::PostgresStore::connect(&url).await?;
            Ok(store)
        }
        other => anyhow::bail!(
            "unsupported BACKEND '{}'. Supported: memory, postgres",
            other
        ),
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
