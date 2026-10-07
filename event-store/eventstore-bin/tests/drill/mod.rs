//! Recovery drill harness (#355).
//!
//! Every drill runs against infrastructure it creates and destroys itself:
//! a disposable Postgres container (`pg`), the real `eventstore-bin` process
//! (`server`), and an in-process TCP fault proxy (`proxy`). Nothing here
//! touches the dev infrastructure or any database it did not create.
//!
//! Drills are `#[ignore]`d so `cargo test` and `make qa*` skip them. Run with
//! `make -C event-store recovery-drill`. See docs/operations/BACKUP-RESTORE.md.
#![allow(dead_code)]

pub mod fixtures;
pub mod pg;
pub mod projection;
pub mod proxy;
pub mod server;
pub mod workload;

use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Upper bound for any single drill step that waits on I/O.
pub const STEP: Duration = Duration::from_secs(60);

/// A name unique within this machine for this run.
pub fn unique(prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!(
        "{prefix}-{}-{nanos}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

/// Poll `f` until it returns `Some`, or panic after `timeout`.
pub async fn eventually<T, F, Fut>(what: &str, timeout: Duration, mut f: F) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Option<T>>,
{
    let start = Instant::now();
    loop {
        if let Some(v) = f().await {
            return v;
        }
        assert!(
            start.elapsed() < timeout,
            "timed out after {timeout:?} waiting for: {what}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Reserve a free local TCP port. The port is released before use, so a
/// collision with another process is possible but unlikely.
pub fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral port")
        .local_addr()
        .unwrap()
        .port()
}
