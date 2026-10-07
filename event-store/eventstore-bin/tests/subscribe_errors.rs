//! A backend subscription failure must reach gRPC clients as a status, not
//! as an empty or silently ended stream (#350).

use std::net::SocketAddr;
use std::time::Duration;

use eventstore_backend_postgres::PostgresStore;
use eventstore_bin::{EventStoreServer, Service};
use eventstore_proto::gen::event_store_client::EventStoreClient;
use eventstore_proto::gen::SubscribeRequest;
use tonic::transport::Server;

#[tokio::test]
async fn postgres_subscription_query_failure_reaches_client_as_unavailable() {
    // A pool that can never connect: the replay query fails.
    let url = "postgres://test:test@127.0.0.1:1/test";
    let pool = sqlx::postgres::PgPoolOptions::new()
        .acquire_timeout(Duration::from_secs(2))
        .connect_lazy(url)
        .expect("lazy pool");
    let store = PostgresStore::new(pool, url.to_owned());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        Server::builder()
            .add_service(EventStoreServer::new(Service { store }))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
    });

    let mut client = EventStoreClient::connect(format!("http://{addr}"))
        .await
        .expect("connect client");
    let mut stream = client
        .subscribe(SubscribeRequest {
            tenant_id: "tenant".into(),
            aggregate_id_prefix: String::new(),
            from_global_nonce: 0,
        })
        .await
        .expect("subscribe call")
        .into_inner();

    let status = tokio::time::timeout(Duration::from_secs(10), stream.message())
        .await
        .expect("no response within timeout")
        .expect_err("a failed replay must not yield a caught-up marker");
    assert_eq!(status.code(), tonic::Code::Unavailable, "{status:?}");
    assert!(
        status.message().contains("resume from global_nonce 0"),
        "{status:?}"
    );

    server.abort();
}

/// Store whose reads all hit an undecodable stored event at global_nonce 9.
struct CorruptStore;

#[tonic::async_trait]
impl eventstore_core::EventStore for CorruptStore {
    async fn append(
        &self,
        _: eventstore_proto::gen::AppendRequest,
    ) -> Result<eventstore_proto::gen::AppendResponse, eventstore_core::StoreError> {
        unimplemented!()
    }
    async fn read_stream(
        &self,
        _: eventstore_proto::gen::ReadStreamRequest,
    ) -> Result<eventstore_proto::gen::ReadStreamResponse, eventstore_core::StoreError> {
        unimplemented!()
    }
    async fn read_all(
        &self,
        _: eventstore_proto::gen::ReadAllRequest,
    ) -> Result<eventstore_proto::gen::ReadAllResponse, eventstore_core::StoreError> {
        Err(eventstore_core::StoreError::UndecodableEvent {
            global_nonce: 9,
            reason: "column 'headers' could not be decoded".into(),
        })
    }
    fn subscribe(
        &self,
        _: eventstore_proto::gen::SubscribeRequest,
    ) -> eventstore_core::StoreStream<eventstore_proto::gen::SubscribeResponse> {
        unimplemented!()
    }
}

#[tokio::test]
async fn undecodable_event_reaches_client_as_data_loss_with_position_metadata() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        Server::builder()
            .add_service(EventStoreServer::new(Service {
                store: std::sync::Arc::new(CorruptStore),
            }))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
    });

    let mut client = EventStoreClient::connect(format!("http://{addr}"))
        .await
        .expect("connect client");
    let status = client
        .read_all(eventstore_proto::gen::ReadAllRequest {
            tenant_id: "tenant".into(),
            from_global_nonce: u64::MAX >> 1,
            max_count: 1,
            forward: false,
        })
        .await
        .expect_err("undecodable head must fail");
    assert_eq!(status.code(), tonic::Code::DataLoss, "{status:?}");
    let nonce = status
        .metadata()
        .get(eventstore_core::errors::UNDECODABLE_GLOBAL_NONCE_KEY)
        .expect("position travels as trailing metadata")
        .to_str()
        .unwrap();
    assert_eq!(nonce, "9");

    server.abort();
}
