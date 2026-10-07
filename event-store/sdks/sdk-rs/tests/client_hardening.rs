//! Live tests for ClientConfig: TLS, https URLs, timeouts, keepalive, auth.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use eventstore_proto::gen::event_store_server::EventStoreServer;
use eventstore_proto::gen::{AppendRequest, EventData, EventMetadata, SubscribeRequest};
use eventstore_sdk_rs::{ClientConfig, EventStore, SharedToken, TlsConfig};
use rcgen::{BasicConstraints, CertificateParams, CertifiedIssuer, IsCa, KeyPair};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::time::{timeout, Duration, Instant};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Certificate, Identity, Server, ServerTlsConfig};
use tonic::{Code, Request, Status};

// ---------------------------------------------------------------- helpers

fn service() -> eventstore_bin::Service {
    eventstore_bin::Service {
        store: eventstore_backend_memory::InMemoryStore::new(),
    }
}

struct TestServer {
    port: u16,
    shutdown: Option<oneshot::Sender<()>>,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

type Seen = Arc<Mutex<Vec<Option<String>>>>;

/// Serve the memory event store on an ephemeral port, recording the
/// `authorization` header of every request.
async fn spawn_server(tls: Option<ServerTlsConfig>) -> (TestServer, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen: Seen = Arc::default();
    let record = seen.clone();
    let svc = EventStoreServer::with_interceptor(service(), move |req: Request<()>| {
        let header = req
            .metadata()
            .get("authorization")
            .map(|v| v.to_str().unwrap().to_string());
        record.lock().unwrap().push(header);
        Ok::<_, Status>(req)
    });
    let (tx, rx) = oneshot::channel::<()>();
    let mut builder = Server::builder();
    if let Some(tls) = tls {
        builder = builder.tls_config(tls).unwrap();
    }
    let router = builder.add_service(svc);
    tokio::spawn(async move {
        let _ = router
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async {
                let _ = rx.await;
            })
            .await;
    });
    (
        TestServer {
            port,
            shutdown: Some(tx),
        },
        seen,
    )
}

fn append_request(aggregate_id: &str, expected: u64) -> AppendRequest {
    AppendRequest {
        tenant_id: "t".into(),
        aggregate_id: aggregate_id.into(),
        aggregate_type: "Order".into(),
        expected_aggregate_nonce: expected,
        idempotency_key: String::new(),
        events: vec![EventData {
            meta: Some(EventMetadata {
                event_id: uuid_like(aggregate_id, expected),
                aggregate_id: aggregate_id.into(),
                aggregate_type: "Order".into(),
                aggregate_nonce: expected + 1,
                event_type: "OrderCreated".into(),
                event_version: 1,
                content_type: "application/json".into(),
                tenant_id: "t".into(),
                ..Default::default()
            }),
            payload: b"{}".to_vec(),
        }],
    }
}

fn uuid_like(a: &str, n: u64) -> String {
    format!("evt-{a}-{n}")
}

/// Test PKI: a CA, a server leaf for `localhost` only (no IP SAN), and a
/// client leaf for mTLS.
struct Pki {
    ca_pem: String,
    server: Identity,
    client_cert: String,
    client_key: String,
}

fn pki() -> Pki {
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let ca = CertifiedIssuer::self_signed(ca_params, KeyPair::generate().unwrap()).unwrap();

    let server_key = KeyPair::generate().unwrap();
    let server_cert = CertificateParams::new(vec!["localhost".to_string()])
        .unwrap()
        .signed_by(&server_key, &ca)
        .unwrap();

    let client_key = KeyPair::generate().unwrap();
    let client_cert = CertificateParams::new(vec!["client".to_string()])
        .unwrap()
        .signed_by(&client_key, &ca)
        .unwrap();

    Pki {
        ca_pem: ca.pem(),
        server: Identity::from_pem(server_cert.pem(), server_key.serialize_pem()),
        client_cert: client_cert.pem(),
        client_key: client_key.serialize_pem(),
    }
}

async fn tls_server(pki: &Pki, require_client_cert: bool) -> TestServer {
    let mut tls = ServerTlsConfig::new().identity(pki.server.clone());
    if require_client_cert {
        tls = tls.client_ca_root(Certificate::from_pem(&pki.ca_pem));
    }
    spawn_server(Some(tls)).await.0
}

/// Connect and do one append; the result says whether the full path
/// (connect, handshake, RPC) works.
async fn try_append(cfg: ClientConfig) -> anyhow::Result<()> {
    let mut store = cfg
        .request_timeout(Duration::from_secs(5))
        .connect()
        .await?;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let id = format!("tls-{}", NEXT.fetch_add(1, Ordering::SeqCst));
    store.append(append_request(&id, 0)).await?;
    Ok(())
}

// ---------------------------------------------------------------- TLS

#[tokio::test]
async fn https_with_custom_ca_succeeds() {
    let pki = pki();
    let server = tls_server(&pki, false).await;
    try_append(
        ClientConfig::new(format!("https://localhost:{}", server.port))
            .tls(TlsConfig::new().ca_certificate_pem(pki.ca_pem.clone())),
    )
    .await
    .expect("TLS append");

    // Bare host:port plus tls() is TLS too.
    try_append(
        ClientConfig::new(format!("localhost:{}", server.port))
            .tls(TlsConfig::new().ca_certificate_pem(pki.ca_pem.clone())),
    )
    .await
    .expect("TLS append via bare host + tls()");
}

#[tokio::test]
async fn untrusted_server_certificate_is_rejected() {
    let pki = pki();
    let server = tls_server(&pki, false).await;
    // OS trust store does not contain the test CA.
    let err = try_append(ClientConfig::new(format!(
        "https://localhost:{}",
        server.port
    )))
    .await
    .expect_err("self-signed CA must not be trusted by default");
    assert!(
        format!("{err:?}").to_lowercase().contains("certificate"),
        "{err:?}"
    );

    // A different CA must not validate it either.
    let other = pki_ca_only();
    try_append(
        ClientConfig::new(format!("https://localhost:{}", server.port))
            .tls(TlsConfig::new().ca_certificate_pem(other)),
    )
    .await
    .expect_err("wrong CA must fail");
}

fn pki_ca_only() -> String {
    pki().ca_pem
}

#[tokio::test]
async fn plaintext_client_cannot_talk_to_tls_server() {
    let pki = pki();
    let server = tls_server(&pki, false).await;
    try_append(ClientConfig::new(format!(
        "http://localhost:{}",
        server.port
    )))
    .await
    .expect_err("plaintext against TLS must fail");
}

#[tokio::test]
async fn domain_override_validates_against_given_name() {
    let pki = pki();
    let server = tls_server(&pki, false).await;
    let by_ip = format!("https://127.0.0.1:{}", server.port);
    // The cert has no IP SAN, so connecting by IP fails...
    try_append(
        ClientConfig::new(by_ip.clone())
            .tls(TlsConfig::new().ca_certificate_pem(pki.ca_pem.clone())),
    )
    .await
    .expect_err("IP not in cert SANs");
    // ...unless the expected name is overridden.
    try_append(
        ClientConfig::new(by_ip).tls(
            TlsConfig::new()
                .ca_certificate_pem(pki.ca_pem.clone())
                .domain_name("localhost"),
        ),
    )
    .await
    .expect("domain override");
}

#[tokio::test]
async fn mutual_tls_requires_client_identity() {
    let pki = pki();
    let server = tls_server(&pki, true).await;
    let url = format!("https://localhost:{}", server.port);
    try_append(
        ClientConfig::new(url.clone()).tls(TlsConfig::new().ca_certificate_pem(pki.ca_pem.clone())),
    )
    .await
    .expect_err("server requires a client cert");
    try_append(
        ClientConfig::new(url).tls(
            TlsConfig::new()
                .ca_certificate_pem(pki.ca_pem.clone())
                .client_identity_pem(pki.client_cert.clone(), pki.client_key.clone()),
        ),
    )
    .await
    .expect("mTLS append");
}

// ---------------------------------------------------------------- auth

#[tokio::test]
async fn auth_headers_reach_the_server() {
    let (server, seen) = spawn_server(None).await;
    let addr = format!("127.0.0.1:{}", server.port);

    let mut plain = EventStore::connect(&addr).await.unwrap();
    plain.append(append_request("p", 0)).await.unwrap();

    let mut basic = ClientConfig::new(addr.clone())
        .basic_auth("Aladdin", "open sesame")
        .connect()
        .await
        .unwrap();
    basic.append(append_request("b", 0)).await.unwrap();

    let mut bearer = ClientConfig::new(addr.clone())
        .bearer_token("static-token")
        .connect()
        .await
        .unwrap();
    bearer.append(append_request("s", 0)).await.unwrap();

    let token = SharedToken::new("v1");
    let mut rotating = ClientConfig::new(addr.clone())
        .token_provider(token.clone())
        .connect()
        .await
        .unwrap();
    rotating.append(append_request("r", 0)).await.unwrap();
    token.set("v2");
    rotating.append(append_request("r", 1)).await.unwrap();
    // Subscriptions carry the header too.
    let _stream = rotating
        .subscribe(SubscribeRequest {
            tenant_id: "t".into(),
            aggregate_id_prefix: String::new(),
            from_global_nonce: 0,
        })
        .await
        .unwrap();

    let seen = seen.lock().unwrap().clone();
    assert_eq!(
        seen,
        vec![
            None,
            Some("Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ==".into()),
            Some("Bearer static-token".into()),
            Some("Bearer v1".into()),
            Some("Bearer v2".into()),
            Some("Bearer v2".into()),
        ]
    );
}

#[tokio::test]
async fn credentials_over_plaintext_to_remote_host_are_refused() {
    let err = ClientConfig::new("203.0.113.7:8081")
        .basic_auth("u", "secret-pw")
        .lazy_connect(true)
        .connect()
        .await
        .expect_err("must refuse");
    let msg = format!("{err:?} {err}");
    assert!(
        msg.contains("plaintext") && !msg.contains("secret-pw"),
        "{msg}"
    );
}

// ---------------------------------------------------------------- timeouts

/// Accepts TCP connections and never reads or writes.
async fn black_hole() -> (u16, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((sock, _)) = listener.accept().await {
            held.push(sock);
        }
    });
    (port, handle)
}

#[tokio::test]
async fn request_timeout_fires_against_black_hole() {
    let (port, hole) = black_hole().await;
    let mut store = ClientConfig::new(format!("127.0.0.1:{port}"))
        .lazy_connect(true)
        .connect_timeout(None)
        .http2_keepalive(None, Duration::from_secs(1))
        .request_timeout(Duration::from_millis(300))
        .connect()
        .await
        .unwrap();
    let start = Instant::now();
    let err = timeout(Duration::from_secs(5), store.append(append_request("x", 0)))
        .await
        .expect("request timeout must fire, not hang")
        .expect_err("black hole cannot answer");
    let status = err.downcast::<Status>().expect("status");
    assert_eq!(status.code(), Code::DeadlineExceeded, "{status:?}");
    assert!(start.elapsed() >= Duration::from_millis(250));
    hole.abort();
}

#[tokio::test]
async fn connect_timeout_fires_against_black_hole() {
    // TCP connects but the TLS handshake stalls (no ServerHello), so an
    // eager connect can only end via the connect timeout.
    let (port, hole) = black_hole().await;
    let start = Instant::now();
    let err = timeout(
        Duration::from_secs(5),
        ClientConfig::new(format!("https://localhost:{port}"))
            .connect_timeout(Duration::from_millis(300))
            .connect(),
    )
    .await
    .expect("connect must not hang")
    .expect_err("stalled handshake must fail");
    let elapsed = start.elapsed();
    assert!(
        elapsed >= Duration::from_millis(250) && elapsed < Duration::from_millis(1500),
        "{elapsed:?} {err:?}"
    );
    hole.abort();
}

#[tokio::test]
async fn request_timeout_does_not_kill_open_subscription() {
    let (server, _) = spawn_server(None).await;
    let addr = format!("127.0.0.1:{}", server.port);
    let mut reader = ClientConfig::new(addr.clone())
        .request_timeout(Duration::from_millis(200))
        .connect()
        .await
        .unwrap();
    let mut stream = reader
        .subscribe(SubscribeRequest {
            tenant_id: "t".into(),
            aggregate_id_prefix: String::new(),
            from_global_nonce: 0,
        })
        .await
        .unwrap();
    // Idle well past the request timeout, then publish.
    tokio::time::sleep(Duration::from_millis(800)).await;
    let mut writer = EventStore::connect(&addr).await.unwrap();
    writer.append(append_request("late", 0)).await.unwrap();
    let msg = timeout(Duration::from_secs(5), async {
        loop {
            let resp = stream
                .message()
                .await
                .expect("stream healthy")
                .expect("open");
            if let Some(ev) = resp.event {
                return ev;
            }
        }
    })
    .await
    .expect("event delivered");
    assert_eq!(msg.meta.unwrap().aggregate_id, "late");
}

// ---------------------------------------------------------------- keepalive

/// TCP proxy to `upstream` that can be frozen: once frozen it stops
/// forwarding in both directions but keeps the sockets open, like a paused
/// VM or a firewall black-hole.
async fn freezable_proxy(upstream: u16) -> (u16, Arc<AtomicBool>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let frozen = Arc::new(AtomicBool::new(false));
    let flag = frozen.clone();
    tokio::spawn(async move {
        while let Ok((client, _)) = listener.accept().await {
            let server = TcpStream::connect(("127.0.0.1", upstream)).await.unwrap();
            let (cr, cw) = client.into_split();
            let (sr, sw) = server.into_split();
            tokio::spawn(pump(cr, sw, flag.clone()));
            tokio::spawn(pump(sr, cw, flag.clone()));
        }
    });
    (port, frozen)
}

async fn pump(
    mut from: tokio::net::tcp::OwnedReadHalf,
    mut to: tokio::net::tcp::OwnedWriteHalf,
    frozen: Arc<AtomicBool>,
) {
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        let n = match from.read(&mut buf).await {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        if frozen.load(Ordering::SeqCst) {
            // Drop the bytes and hold both halves open forever.
            std::future::pending::<()>().await;
        }
        if to.write_all(&buf[..n]).await.is_err() {
            return;
        }
    }
}

async fn subscribe_then_freeze(
    keepalive: Option<Duration>,
) -> tonic::Streaming<eventstore_proto::gen::SubscribeResponse> {
    let (server, _) = spawn_server(None).await;
    let (proxy, frozen) = freezable_proxy(server.port).await;
    let mut store = ClientConfig::new(format!("127.0.0.1:{proxy}"))
        .http2_keepalive(keepalive, Duration::from_millis(300))
        .request_timeout(Duration::from_secs(5))
        .connect()
        .await
        .unwrap();
    let stream = store
        .subscribe(SubscribeRequest {
            tenant_id: "t".into(),
            aggregate_id_prefix: String::new(),
            from_global_nonce: 0,
        })
        .await
        .unwrap();
    frozen.store(true, Ordering::SeqCst);
    // The server must stay up for the stream's lifetime.
    std::mem::forget(server);
    stream
}

#[tokio::test]
async fn http2_keepalive_ends_subscription_on_dead_connection() {
    let mut stream = subscribe_then_freeze(Some(Duration::from_millis(200))).await;
    let res = timeout(Duration::from_secs(5), stream.message())
        .await
        .expect("keepalive must surface the dead connection");
    let status = res.expect_err("dead connection is an error, not end of stream");
    assert!(
        matches!(
            status.code(),
            Code::Unavailable | Code::Unknown | Code::Cancelled
        ),
        "{status:?}"
    );
}

#[tokio::test]
async fn without_keepalive_dead_subscription_hangs() {
    // Control for the test above: proves the failure comes from keepalive,
    // not from the proxy closing the socket.
    let mut stream = subscribe_then_freeze(None).await;
    assert!(
        timeout(Duration::from_millis(1500), stream.message())
            .await
            .is_err(),
        "without keepalive the frozen stream must stay pending"
    );
}
