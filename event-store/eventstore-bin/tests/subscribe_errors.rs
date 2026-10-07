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
