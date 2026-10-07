//! The real `eventstore-bin` process, started with `BACKEND=postgres` and
//! killed with SIGKILL (no graceful shutdown, no drop handlers).

use std::fs::{File, OpenOptions};
use std::io::Read;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use eventstore_proto::gen::event_store_client::EventStoreClient;
use eventstore_proto::gen::ReadStreamRequest;
use tonic::transport::Channel;

use super::{eventually, free_port, unique, STEP};

pub struct EventStoreProc {
    child: Option<Child>,
    pub addr: SocketAddr,
    database_url: String,
    log_path: PathBuf,
}

impl EventStoreProc {
    pub async fn start(database_url: &str) -> Self {
        let mut proc = Self {
            child: None,
            addr: SocketAddr::from(([127, 0, 0, 1], free_port())),
            database_url: database_url.to_owned(),
            log_path: log_path(),
        };
        proc.spawn();
        proc.wait_ready().await;
        proc
    }

    fn spawn(&mut self) {
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
            .expect("open server log");
        let child = Command::new(env!("CARGO_BIN_EXE_eventstore-bin"))
            .env("BACKEND", "postgres")
            .env("DATABASE_URL", &self.database_url)
            .env("BIND_ADDR", self.addr.to_string())
            .env("RUST_LOG", "info")
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .expect("spawn eventstore-bin");
        self.child = Some(child);
    }

    /// Wait until the process serves RPCs (migrations done, port bound).
    pub async fn wait_ready(&mut self) {
        let endpoint = self.endpoint();
        let start = std::time::Instant::now();
        loop {
            if let Some(status) = self.child.as_mut().and_then(|c| c.try_wait().unwrap()) {
                panic!(
                    "eventstore-bin exited during startup ({status}); log:\n{}",
                    self.log_tail()
                );
            }
            if probe(&endpoint).await {
                return;
            }
            assert!(
                start.elapsed() < STEP,
                "eventstore-bin not ready after {STEP:?}; log:\n{}",
                self.log_tail()
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    pub fn pid(&self) -> u32 {
        self.child.as_ref().expect("process running").id()
    }

    /// SIGKILL the process and reap it.
    pub fn kill(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Start a new process on the same address and database.
    pub async fn restart(&mut self) {
        self.kill();
        self.spawn();
        self.wait_ready().await;
    }

    pub fn endpoint(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub async fn client(&self) -> EventStoreClient<Channel> {
        connect(&self.endpoint()).await
    }

    pub fn log_tail(&self) -> String {
        let mut s = String::new();
        if let Ok(mut f) = File::open(&self.log_path) {
            let _ = f.read_to_string(&mut s);
        }
        let start = s.len().saturating_sub(4000);
        let start = (start..s.len())
            .find(|i| s.is_char_boundary(*i))
            .unwrap_or(0);
        s[start..].to_owned()
    }
}

impl Drop for EventStoreProc {
    fn drop(&mut self) {
        self.kill();
    }
}

fn log_path() -> PathBuf {
    let dir = std::env::temp_dir().join("esp-recovery-drill-logs");
    std::fs::create_dir_all(&dir).expect("create log dir");
    dir.join(format!("{}.log", unique("eventstore")))
}

pub async fn connect(endpoint: &str) -> EventStoreClient<Channel> {
    let endpoint = endpoint.to_owned();
    eventually(&format!("gRPC connect {endpoint}"), STEP, || {
        let endpoint = endpoint.clone();
        async move { EventStoreClient::connect(endpoint).await.ok() }
    })
    .await
}

async fn probe(endpoint: &str) -> bool {
    let Ok(mut client) = EventStoreClient::connect(endpoint.to_owned()).await else {
        return false;
    };
    let req = ReadStreamRequest {
        tenant_id: "drill-probe".into(),
        aggregate_id: "probe".into(),
        from_aggregate_nonce: 1,
        max_count: 1,
        forward: true,
    };
    matches!(
        tokio::time::timeout(Duration::from_secs(5), client.read_stream(req)).await,
        Ok(Ok(_))
    )
}
