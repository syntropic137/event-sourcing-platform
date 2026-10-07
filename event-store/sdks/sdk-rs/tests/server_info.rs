//! Live-server tests for the connect-time compatibility helpers.

use std::net::SocketAddr;
use std::pin::Pin;

use eventstore_proto::gen::event_store_server::{EventStore as EventStoreRpc, EventStoreServer};
use eventstore_proto::gen::{
    AppendRequest, AppendResponse, GetServerInfoRequest, GetServerInfoResponse, ReadAllRequest,
    ReadAllResponse, ReadStreamRequest, ReadStreamResponse, SubscribeRequest, SubscribeResponse,
};
use eventstore_sdk_rs::{capabilities, CompatibilityError, EventStore};
use tokio::sync::oneshot;
use tokio::time::{sleep, Duration, Instant};
use tokio_stream::Stream;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

/// The real server, except `GetServerInfo` answers `UNIMPLEMENTED`, which is
/// exactly what a pre-0.17.0 server returns for a method it does not know.
struct LegacyServer(eventstore_bin::Service);

#[tonic::async_trait]
impl EventStoreRpc for LegacyServer {
    async fn append(&self, r: Request<AppendRequest>) -> Result<Response<AppendResponse>, Status> {
        self.0.append(r).await
    }
    async fn read_stream(
        &self,
        r: Request<ReadStreamRequest>,
    ) -> Result<Response<ReadStreamResponse>, Status> {
        self.0.read_stream(r).await
    }
    async fn read_all(
        &self,
        r: Request<ReadAllRequest>,
    ) -> Result<Response<ReadAllResponse>, Status> {
        self.0.read_all(r).await
    }
    async fn get_server_info(
        &self,
        _r: Request<GetServerInfoRequest>,
    ) -> Result<Response<GetServerInfoResponse>, Status> {
        Err(Status::unimplemented("Method not found"))
    }

    type SubscribeStream =
        Pin<Box<dyn Stream<Item = Result<SubscribeResponse, Status>> + Send + 'static>>;

    async fn subscribe(
        &self,
        r: Request<SubscribeRequest>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        self.0.subscribe(r).await
    }
}

fn memory_service() -> eventstore_bin::Service {
    eventstore_bin::Service {
        store: eventstore_backend_memory::InMemoryStore::new(),
    }
}

async fn spawn<S>(svc: S) -> (String, oneshot::Sender<()>, tokio::task::JoinHandle<()>)
where
    S: EventStoreRpc,
{
    let port = portpicker::pick_unused_port().expect("free port");
    let addr = format!("127.0.0.1:{port}");
    let socket: SocketAddr = addr.parse().unwrap();
    let (tx, rx) = oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        let _ = Server::builder()
            .add_service(EventStoreServer::new(svc))
            .serve_with_shutdown(socket, async {
                let _ = rx.await;
            })
            .await;
    });
    (addr, tx, handle)
}

async fn connect(addr: &str) -> EventStore {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match EventStore::connect(addr).await {
            Ok(c) => return c,
            Err(e) if Instant::now() >= deadline => panic!("connect {addr}: {e:?}"),
            Err(_) => sleep(Duration::from_millis(50)).await,
        }
    }
}

#[tokio::test]
async fn current_server_reports_info_and_meets_floor() {
    let (addr, shutdown, handle) = spawn(memory_service()).await;
    let mut client = connect(&addr).await;

    let info = client.server_info().await.expect("server_info");
    assert!(!info.is_legacy());
    assert_eq!(
        info.server_version.as_deref(),
        Some(eventstore_bin::SERVER_VERSION)
    );
    assert_eq!(info.api_version.as_deref(), Some("eventstore.v1"));
    assert_eq!(info.backend.as_deref(), Some("memory"));
    assert!(info.has_capability(capabilities::COMMIT_ORDERED_GLOBAL_NONCE));

    client
        .require_capabilities(&[capabilities::COMMIT_ORDERED_GLOBAL_NONCE])
        .await
        .expect("current server has the #337 guarantee");
    client
        .require_min_version("0.16.0")
        .await
        .expect("current server is >= 0.16.0");

    let err = client
        .require_capabilities(&["not_a_real_capability"])
        .await
        .expect_err("unknown capability must be missing");
    assert_eq!(
        err.downcast_ref::<CompatibilityError>(),
        Some(&CompatibilityError::MissingCapabilities {
            server_version: Some(eventstore_bin::SERVER_VERSION.to_string()),
            missing: vec!["not_a_real_capability".into()],
        })
    );

    drop(client);
    let _ = shutdown.send(());
    let _ = handle.await;
}

#[tokio::test]
async fn legacy_server_is_treated_as_lacking_every_capability() {
    let (addr, shutdown, handle) = spawn(LegacyServer(memory_service())).await;
    let mut client = connect(&addr).await;

    let info = client
        .server_info()
        .await
        .expect("UNIMPLEMENTED maps to legacy info, not an error");
    assert!(info.is_legacy());
    assert!(info.capabilities.is_empty());

    let err = client
        .require_capabilities(&[capabilities::COMMIT_ORDERED_GLOBAL_NONCE])
        .await
        .expect_err("legacy server cannot prove the #337 guarantee");
    assert!(matches!(
        err.downcast_ref::<CompatibilityError>(),
        Some(CompatibilityError::MissingCapabilities {
            server_version: None,
            ..
        })
    ));

    let err = client
        .require_min_version("0.16.0")
        .await
        .expect_err("legacy server version is unknown");
    assert!(matches!(
        err.downcast_ref::<CompatibilityError>(),
        Some(CompatibilityError::VersionTooOld { .. })
    ));

    // An empty requirement is trivially met, even by a legacy server.
    client.require_capabilities(&[]).await.expect("empty floor");

    drop(client);
    let _ = shutdown.send(());
    let _ = handle.await;
}

#[tokio::test]
async fn transport_errors_are_not_mistaken_for_legacy() {
    // Server that rejects GetServerInfo with a non-UNIMPLEMENTED status.
    struct Broken(eventstore_bin::Service);
    #[tonic::async_trait]
    impl EventStoreRpc for Broken {
        async fn append(
            &self,
            r: Request<AppendRequest>,
        ) -> Result<Response<AppendResponse>, Status> {
            self.0.append(r).await
        }
        async fn read_stream(
            &self,
            r: Request<ReadStreamRequest>,
        ) -> Result<Response<ReadStreamResponse>, Status> {
            self.0.read_stream(r).await
        }
        async fn read_all(
            &self,
            r: Request<ReadAllRequest>,
        ) -> Result<Response<ReadAllResponse>, Status> {
            self.0.read_all(r).await
        }
        async fn get_server_info(
            &self,
            _r: Request<GetServerInfoRequest>,
        ) -> Result<Response<GetServerInfoResponse>, Status> {
            Err(Status::unavailable("down"))
        }
        type SubscribeStream =
            Pin<Box<dyn Stream<Item = Result<SubscribeResponse, Status>> + Send + 'static>>;
        async fn subscribe(
            &self,
            r: Request<SubscribeRequest>,
        ) -> Result<Response<Self::SubscribeStream>, Status> {
            self.0.subscribe(r).await
        }
    }

    let (addr, shutdown, handle) = spawn(Broken(memory_service())).await;
    let mut client = connect(&addr).await;
    let err = client
        .server_info()
        .await
        .expect_err("UNAVAILABLE must error");
    let status = err.downcast_ref::<Status>().expect("tonic status");
    assert_eq!(status.code(), tonic::Code::Unavailable);

    drop(client);
    let _ = shutdown.send(());
    let _ = handle.await;
}
