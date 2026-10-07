//! In-process TCP fault proxy between a gRPC client and the event store
//! (after `event-store/eventstore-bin/tests/drill/proxy.rs`).
//!
//! - `cut()`: close every proxied connection and refuse new ones (outage),
//! - `restore()`: accept and forward again.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinHandle;

struct Inner {
    refuse: AtomicBool,
    cut: watch::Sender<u64>,
}

pub struct FaultProxy {
    inner: Arc<Inner>,
    pub addr: String,
    accept: JoinHandle<()>,
}

impl FaultProxy {
    /// Proxy to `upstream` (`host:port`).
    pub async fn start(upstream: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind proxy");
        let addr = listener.local_addr().unwrap().to_string();
        let inner = Arc::new(Inner {
            refuse: AtomicBool::new(false),
            cut: watch::channel(0).0,
        });
        let accept_inner = inner.clone();
        let upstream = upstream.to_string();
        let accept = tokio::spawn(async move {
            loop {
                let Ok((inbound, _)) = listener.accept().await else {
                    return;
                };
                // Subscribe before checking the mode: a cut after this point
                // is seen by `changed()`, a cut before it by the check.
                let cut_rx = accept_inner.cut.subscribe();
                if accept_inner.refuse.load(Ordering::SeqCst) {
                    drop(inbound);
                    continue;
                }
                tokio::spawn(handle(inbound, upstream.clone(), cut_rx));
            }
        });
        Self {
            inner,
            addr,
            accept,
        }
    }

    /// Close all proxied connections and refuse new ones until `restore()`.
    pub fn cut(&self) {
        self.inner.refuse.store(true, Ordering::SeqCst);
        self.inner.cut.send_modify(|g| *g += 1);
    }

    pub fn restore(&self) {
        self.inner.refuse.store(false, Ordering::SeqCst);
    }
}

impl Drop for FaultProxy {
    fn drop(&mut self) {
        self.accept.abort();
        self.cut();
    }
}

async fn handle(inbound: TcpStream, upstream: String, mut cut_rx: watch::Receiver<u64>) {
    let Ok(outbound) = TcpStream::connect(&upstream).await else {
        return;
    };
    let _ = inbound.set_nodelay(true);
    let _ = outbound.set_nodelay(true);
    let (ri, wi) = inbound.into_split();
    let (ro, wo) = outbound.into_split();
    // Whichever finishes first (EOF, error or cut) drops both sockets.
    tokio::select! {
        _ = pump(ri, wo) => {}
        _ = pump(ro, wi) => {}
        _ = cut_rx.changed() => {}
    }
}

async fn pump(mut r: OwnedReadHalf, mut w: OwnedWriteHalf) {
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        let n = match r.read(&mut buf).await {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        if w.write_all(&buf[..n]).await.is_err() {
            return;
        }
    }
}
