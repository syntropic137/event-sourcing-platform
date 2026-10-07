//! Live tests for EventStoreClient connection config and compatibility checks.

mod common;

use std::time::Duration;

use event_sourcing_rust::client::{
    capabilities, proto, ClientConfig, EventStoreClient, EventStorePort,
};
use event_sourcing_rust::error::Error;
use tokio::net::TcpListener;

fn read_all(tenant: &str) -> proto::ReadAllRequest {
    proto::ReadAllRequest {
        tenant_id: tenant.into(),
        from_global_nonce: 0,
        max_count: 10,
        forward: true,
    }
}

#[tokio::test]
async fn server_info_and_capability_checks() {
    let server = common::spawn_server().await;
    let client = common::connect(&server.addr).await;

    let info = client.server_info().await.unwrap();
    assert!(!info.is_legacy());
    assert!(info.has_capability(capabilities::COMMIT_ORDERED_GLOBAL_NONCE));

    client
        .require_capabilities(&[capabilities::COMMIT_ORDERED_GLOBAL_NONCE])
        .await
        .unwrap();
    client.require_min_version("0.17.0").await.unwrap();

    let err = client
        .require_capabilities(&["no_such_capability"])
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Incompatible(_)), "{err:?}");
    assert!(err.to_string().contains("no_such_capability"));
    let err = client.require_min_version("999.0.0").await.unwrap_err();
    assert!(matches!(err, Error::Incompatible(_)), "{err:?}");
}

#[tokio::test]
async fn http_url_and_config_connect() {
    let server = common::spawn_server().await;
    // Wait until the server is up.
    let _ = common::connect(&server.addr).await;

    let by_url = EventStoreClient::connect(format!("http://{}", server.addr))
        .await
        .unwrap();
    by_url.read_all(read_all("t")).await.unwrap();

    let configured = EventStoreClient::connect_with(
        ClientConfig::new(server.addr.clone())
            .bearer_token("tok")
            .request_timeout(Duration::from_secs(5)),
    )
    .await
    .unwrap();
    configured.read_all(read_all("t")).await.unwrap();
}

#[tokio::test]
async fn https_url_uses_tls_not_a_doubled_scheme() {
    // A plaintext server: TLS handshake must fail as a transport error. The
    // old client turned this URL into "http://https://..." (a parse error).
    let server = common::spawn_server().await;
    let _ = common::connect(&server.addr).await;
    let err = EventStoreClient::connect(format!("https://{}", server.addr))
        .await
        .unwrap_err();
    assert!(err.is_transient(), "{err:?}");
}

#[tokio::test]
async fn config_errors_are_typed_and_redacted() {
    let err = EventStoreClient::connect_with(
        ClientConfig::new("db.example.com:8081").basic_auth("u", "hunter2"),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, Error::Config(_)), "{err:?}");
    assert!(!format!("{err:?}").contains("hunter2"));

    let err = EventStoreClient::connect("ftp://x:1").await.unwrap_err();
    assert!(matches!(err, Error::Config(_)), "{err:?}");
}

#[tokio::test]
async fn request_timeout_is_transient_deadline_exceeded() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let hole = tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((s, _)) = listener.accept().await {
            held.push(s);
        }
    });
    let client = EventStoreClient::connect_with(
        ClientConfig::new(format!("127.0.0.1:{port}"))
            .lazy_connect(true)
            .request_timeout(Duration::from_millis(200)),
    )
    .await
    .unwrap();
    let err = tokio::time::timeout(Duration::from_secs(5), client.read_all(read_all("t")))
        .await
        .expect("must not hang")
        .unwrap_err();
    assert_eq!(
        err.status_code(),
        Some(tonic::Code::DeadlineExceeded),
        "{err:?}"
    );
    assert!(err.is_transient());
    hole.abort();
}
