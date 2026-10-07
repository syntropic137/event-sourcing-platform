//! In-process TCP fault proxy.
//!
//! Sits between a client and a server (the event store and Postgres, or a
//! gRPC client and the event store) so a drill can deterministically:
//!
//! - `hold()`: keep connections open but stop forwarding in both directions
//!   (a request is "in flight" and cannot complete),
//! - `hold_responses()`: forward requests but not responses (the server acts,
//!   the client never sees the acknowledgment),
//! - `cut()`: close every proxied connection and reject new ones (outage),
//! - `restore()`: forward normally again.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinHandle;

const PASS: u8 = 0;
const HOLD: u8 = 1;
const HOLD_RESPONSES: u8 = 2;
const REFUSE: u8 = 3;

#[derive(Clone, Copy, PartialEq)]
enum Dir {
    ClientToServer,
    ServerToClient,
}

struct Inner {
    mode: AtomicU8,
    cut: watch::Sender<u64>,
    c2s_bytes: AtomicU64,
    s2c_bytes: AtomicU64,
}

impl Inner {
    fn holds(&self, dir: Dir) -> bool {
        match self.mode.load(Ordering::SeqCst) {
            PASS => false,
            HOLD | REFUSE => true,
            HOLD_RESPONSES => dir == Dir::ServerToClient,
            _ => unreachable!(),
        }
    }
}

pub struct FaultProxy {
    inner: Arc<Inner>,
    pub addr: SocketAddr,
    accept: JoinHandle<()>,
}

impl FaultProxy {
    pub async fn start(upstream: SocketAddr) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind proxy");
        let addr = listener.local_addr().unwrap();
        let (cut, _) = watch::channel(0u64);
        let inner = Arc::new(Inner {
            mode: AtomicU8::new(PASS),
            cut,
            c2s_bytes: AtomicU64::new(0),
            s2c_bytes: AtomicU64::new(0),
        });
        let accept_inner = inner.clone();
        let accept = tokio::spawn(async move {
            loop {
                let Ok((inbound, _)) = listener.accept().await else {
                    return;
                };
                // Subscribe before checking the mode: a cut after this point
                // is seen by `changed()`, a cut before it by the mode check.
                let cut_rx = accept_inner.cut.subscribe();
                if accept_inner.mode.load(Ordering::SeqCst) == REFUSE {
                    drop(inbound);
                    continue;
                }
                tokio::spawn(handle(accept_inner.clone(), inbound, upstream, cut_rx));
            }
        });
        Self {
            inner,
            addr,
            accept,
        }
    }

    /// `postgres://...` URL pointing at this proxy instead of `upstream_url`'s
    /// host:port.
    pub fn rewrite_url(&self, upstream_url: &str, upstream: SocketAddr) -> String {
        upstream_url.replace(&upstream.to_string(), &self.addr.to_string())
    }

    pub fn endpoint(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn hold(&self) {
        self.inner.mode.store(HOLD, Ordering::SeqCst);
    }

    pub fn hold_responses(&self) {
        self.inner.mode.store(HOLD_RESPONSES, Ordering::SeqCst);
    }

    /// Close all proxied connections and refuse new ones until `restore()`.
    pub fn cut(&self) {
        self.inner.mode.store(REFUSE, Ordering::SeqCst);
        self.inner.cut.send_modify(|g| *g += 1);
    }

    pub fn restore(&self) {
        self.inner.mode.store(PASS, Ordering::SeqCst);
    }

    /// Bytes received from clients so far (forwarded or held).
    pub fn client_bytes(&self) -> u64 {
        self.inner.c2s_bytes.load(Ordering::SeqCst)
    }

    /// Bytes received from the server so far (forwarded or held).
    pub fn server_bytes(&self) -> u64 {
        self.inner.s2c_bytes.load(Ordering::SeqCst)
    }
}

impl Drop for FaultProxy {
    fn drop(&mut self) {
        self.accept.abort();
        self.cut();
    }
}

async fn handle(
    inner: Arc<Inner>,
    inbound: TcpStream,
    upstream: SocketAddr,
    mut cut_rx: watch::Receiver<u64>,
) {
    let Ok(outbound) = TcpStream::connect(upstream).await else {
        return;
    };
    let _ = inbound.set_nodelay(true);
    let _ = outbound.set_nodelay(true);
    let (ri, wi) = inbound.into_split();
    let (ro, wo) = outbound.into_split();
    // Whichever finishes first (EOF, error or cut) drops both sockets.
    tokio::select! {
        _ = pump(inner.clone(), ri, wo, Dir::ClientToServer) => {}
        _ = pump(inner.clone(), ro, wi, Dir::ServerToClient) => {}
        _ = cut_rx.changed() => {}
    }
}

async fn pump(inner: Arc<Inner>, mut r: OwnedReadHalf, mut w: OwnedWriteHalf, dir: Dir) {
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        let n = match r.read(&mut buf).await {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        let counter = match dir {
            Dir::ClientToServer => &inner.c2s_bytes,
            Dir::ServerToClient => &inner.s2c_bytes,
        };
        counter.fetch_add(n as u64, Ordering::SeqCst);
        while inner.holds(dir) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        if w.write_all(&buf[..n]).await.is_err() {
            return;
        }
    }
}
