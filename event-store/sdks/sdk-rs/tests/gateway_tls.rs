//! End-to-end: Rust SDK -> nginx gateway (TLS + Basic Auth, #301) -> event
//! store. Needs Docker and a built gateway image, so it is `#[ignore]`d; run
//! via `make -C event-store gateway-tls-e2e`, which builds the image and sets
//! `ESP_GATEWAY_E2E_IMAGE`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use eventstore_proto::gen::event_store_server::EventStoreServer;
use eventstore_proto::gen::{AppendRequest, EventData, EventMetadata, SubscribeRequest};
use eventstore_sdk_rs::{ClientConfig, EventStore, TlsConfig};
use rcgen::{BasicConstraints, CertificateParams, CertifiedIssuer, IsCa, KeyPair};
use tokio::net::TcpListener;
use tokio::time::{sleep, timeout, Duration, Instant};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;
use tonic::{Code, Status};

const PASSWORD: &str = "e2e-gateway-pw";

fn image() -> String {
    std::env::var("ESP_GATEWAY_E2E_IMAGE")
        .expect("ESP_GATEWAY_E2E_IMAGE not set; run `make -C event-store gateway-tls-e2e`")
}

fn docker(args: &[&str]) -> std::process::Output {
    Command::new("docker").args(args).output().expect("docker")
}

/// Removes the container on drop, printing its logs if the test failed.
struct Container(String);

impl Drop for Container {
    fn drop(&mut self) {
        if std::thread::panicking() {
            let out = docker(&["logs", &self.0]);
            eprintln!(
                "--- gateway logs ---\n{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        let _ = docker(&["rm", "-f", &self.0]);
    }
}

/// CA PEM and a `localhost` server cert/key signed by it.
fn pki() -> (String, String, String) {
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let ca = CertifiedIssuer::self_signed(ca_params, KeyPair::generate().unwrap()).unwrap();
    let key = KeyPair::generate().unwrap();
    let cert = CertificateParams::new(vec!["localhost".to_string()])
        .unwrap()
        .signed_by(&key, &ca)
        .unwrap();
    (ca.pem(), cert.pem(), key.serialize_pem())
}

fn write_pair(dir: &Path, cert: &str, key: &str) {
    // Write then rename, like certbot/cert-manager, so the watcher never
    // sees a half-written file.
    for (name, body) in [("fullchain.pem", cert), ("privkey.pem", key)] {
        let tmp = dir.join(format!(".{name}.tmp"));
        std::fs::write(&tmp, body).unwrap();
        std::fs::rename(&tmp, dir.join(name)).unwrap();
    }
}

fn tls_dir(tag: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("gateway-tls-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// In-process memory event store on all interfaces (the container reaches it
/// via the Docker host-gateway IP). Returns its port.
async fn spawn_store() -> u16 {
    let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let svc = EventStoreServer::new(eventstore_bin::Service {
        store: eventstore_backend_memory::InMemoryStore::new(),
    });
    tokio::spawn(
        Server::builder()
            .add_service(svc)
            .serve_with_incoming(TcpListenerStream::new(listener)),
    );
    port
}

/// The Docker host as seen from a container (`host-gateway`).
fn host_ip() -> String {
    let out = docker(&[
        "run",
        "--rm",
        "--add-host",
        "host.docker.internal:host-gateway",
        "--entrypoint",
        "sh",
        &image(),
        "-c",
        "awk '/host.docker.internal/ {print $1; exit}' /etc/hosts",
    ]);
    let ip = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert!(!ip.is_empty(), "host-gateway lookup: {out:?}");
    ip
}

/// Start the gateway with TLS on, return the container and its host port.
fn start_gateway(store_port: u16, dir: &Path) -> (Container, u16) {
    let name = format!("esp-gateway-tls-e2e-{}-{store_port}", std::process::id());
    let mount = format!("{}:/etc/nginx/tls:ro", dir.display());
    // An IP, not a name: nginx resolves `set $upstream` names via Docker's
    // embedded DNS, which the default bridge network does not have.
    let upstream = format!("ESP_UPSTREAM={}:{store_port}", host_ip());
    let password = format!("ESP_GATEWAY_PASSWORD={PASSWORD}");
    let img = image();
    let out = docker(&[
        "run",
        "-d",
        "--name",
        &name,
        "-e",
        &upstream,
        "-e",
        &password,
        "-e",
        "ESP_GATEWAY_TLS_RELOAD_INTERVAL=1",
        "-v",
        &mount,
        "-p",
        "127.0.0.1::8081",
        &img,
    ]);
    assert!(out.status.success(), "docker run: {out:?}");
    let container = Container(name.clone());
    let out = docker(&["port", &name, "8081/tcp"]);
    let mapped = String::from_utf8_lossy(&out.stdout);
    let port = mapped
        .lines()
        .next()
        .and_then(|l| l.rsplit(':').next())
        .and_then(|p| p.trim().parse().ok())
        .unwrap_or_else(|| panic!("docker port: {out:?}"));
    (container, port)
}

fn client(port: u16, ca: &str) -> ClientConfig {
    ClientConfig::new(format!("https://localhost:{port}"))
        .tls(TlsConfig::new().ca_certificate_pem(ca.to_string()))
        .basic_auth("admin", PASSWORD)
        .request_timeout(Duration::from_secs(5))
}

fn append(aggregate_id: &str, expected: u64) -> AppendRequest {
    AppendRequest {
        tenant_id: "t".into(),
        aggregate_id: aggregate_id.into(),
        aggregate_type: "Order".into(),
        expected_aggregate_nonce: expected,
        idempotency_key: String::new(),
        events: vec![EventData {
            meta: Some(EventMetadata {
                event_id: format!("evt-{aggregate_id}-{expected}"),
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

/// Retry until the gateway answers an authenticated append over TLS.
async fn wait_ready(cfg: impl Fn() -> ClientConfig) -> EventStore {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let attempt = async {
            let mut store = cfg().connect().await?;
            store.append(append(&format!("ready-{n}"), 0)).await?;
            anyhow::Ok(store)
        };
        match attempt.await {
            Ok(store) => return store,
            Err(e) if Instant::now() > deadline => panic!("gateway never became ready: {e:?}"),
            Err(_) => {
                sleep(Duration::from_millis(250)).await;
            }
        }
    }
}

fn status(err: anyhow::Error) -> Status {
    err.downcast::<Status>().expect("tonic status")
}

#[tokio::test]
#[ignore = "needs Docker + gateway image; make -C event-store gateway-tls-e2e"]
async fn sdk_through_tls_gateway() {
    let store_port = spawn_store().await;
    let dir = tls_dir("main");
    let (ca, cert, key) = pki();
    write_pair(&dir, &cert, &key);
    let (_gw, port) = start_gateway(store_port, &dir);

    // Authenticated unary call over TLS with a custom (self-signed) CA.
    let mut store = wait_ready(|| client(port, &ca)).await;

    // Server streaming through TLS: an event appended after subscribing is
    // delivered on the open stream.
    let mut sub = store
        .subscribe(SubscribeRequest {
            tenant_id: "t".into(),
            aggregate_id_prefix: "live-".into(),
            from_global_nonce: 0,
        })
        .await
        .expect("subscribe");
    store.append(append("live-1", 0)).await.expect("append");
    let msg = timeout(Duration::from_secs(10), sub.message())
        .await
        .expect("event within 10s")
        .expect("stream ok")
        .expect("stream open");
    assert_eq!(msg.event.unwrap().meta.unwrap().aggregate_id, "live-1");

    // Wrong password: UNAUTHENTICATED, not a transport error.
    let mut bad = ClientConfig::new(format!("https://localhost:{port}"))
        .tls(TlsConfig::new().ca_certificate_pem(ca.clone()))
        .basic_auth("admin", "wrong")
        .connect()
        .await
        .expect("TLS connect");
    let err = bad
        .append(append("bad", 0))
        .await
        .expect_err("bad password");
    assert_eq!(status(err).code(), Code::Unauthenticated);

    // No plaintext path: an h2c client against the TLS port fails.
    let plain = async {
        let mut s = ClientConfig::new(format!("http://localhost:{port}"))
            .basic_auth("admin", PASSWORD)
            .request_timeout(Duration::from_secs(5))
            .connect()
            .await?;
        s.append(append("plain", 0)).await?;
        anyhow::Ok(())
    };
    plain
        .await
        .expect_err("plaintext must not work on the TLS port");

    // Without the custom CA (OS roots only) the self-signed cert is rejected.
    ClientConfig::new(format!("https://localhost:{port}"))
        .basic_auth("admin", PASSWORD)
        .connect()
        .await
        .expect_err("untrusted CA must fail");

    // Rotation: a new pair from a new CA is picked up without a restart.
    let (ca2, cert2, key2) = pki();
    write_pair(&dir, &cert2, &key2);
    let mut rotated = wait_ready(|| client(port, &ca2)).await;
    rotated.append(append("after-rotate", 0)).await.unwrap();
    client(port, &ca)
        .connect()
        .await
        .expect_err("old CA must no longer validate the rotated cert");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
#[ignore = "needs Docker + gateway image; make -C event-store gateway-tls-e2e"]
async fn gateway_fails_closed_without_certificate() {
    let img = image();
    // Default mode is TLS on; no cert mounted -> nginx never starts.
    let out = docker(&["run", "--rm", &img]);
    assert!(!out.status.success(), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("certificate"),
        "{out:?}"
    );

    // An unreadable/garbage pair also fails at startup, not on first request.
    let dir = tls_dir("garbage");
    write_pair(&dir, "not a cert", "not a key");
    let mount = format!("{}:/etc/nginx/tls:ro", dir.display());
    let out = docker(&["run", "--rm", "-v", &mount, &img]);
    assert!(!out.status.success(), "{out:?}");
    let _ = std::fs::remove_dir_all(&dir);

    // Plaintext on a non-loopback publish address is refused.
    let out = docker(&[
        "run",
        "--rm",
        "-e",
        "ESP_GATEWAY_TLS=off",
        "-e",
        "ESP_GATEWAY_PUBLISH_BIND=0.0.0.0",
        &img,
    ]);
    assert!(!out.status.success(), "{out:?}");
}
