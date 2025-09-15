use std::{env, net::SocketAddr};

use anyhow::Context;
use eventstore_bin::{resolve_backend, EventStoreServer, Service};
use tower_http::trace::TraceLayer;
use tracing::{error, info};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let addr: SocketAddr = env::var("BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:50051".to_string())
        .parse()
        .context("invalid BIND_ADDR")?;

    let store = resolve_backend().await?;
    let svc = Service { store };

    info!(%addr, backend=%env::var("BACKEND").unwrap_or_else(|_| "memory".into()), "starting EventStore server");

    tonic::transport::Server::builder()
        .layer(TraceLayer::new_for_grpc())
        .add_service(EventStoreServer::new(svc))
        .serve(addr)
        .await
        .map_err(|e| {
            error!(error = %e, "server error");
            e
        })?;

    Ok(())
}
