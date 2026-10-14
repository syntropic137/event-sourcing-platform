//! Live-server tests for the paged readers (`stream_pages`, `all_pages`).

use std::net::SocketAddr;
use std::pin::Pin;

use eventstore_proto::gen::event_store_server::{EventStore as EventStoreRpc, EventStoreServer};
use eventstore_proto::gen::{
    AppendRequest, AppendResponse, EventData, EventMetadata, GetServerInfoRequest,
    GetServerInfoResponse, ReadAllRequest, ReadAllResponse, ReadStreamRequest, ReadStreamResponse,
    SubscribeRequest, SubscribeResponse,
};
use eventstore_sdk_rs::{EventStore, PagingStalled};
use tokio::sync::oneshot;
use tokio::time::{sleep, Duration, Instant};
use tokio_stream::Stream;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

const TENANT: &str = "t";

fn memory_service() -> eventstore_bin::Service {
    eventstore_bin::Service {
        store: eventstore_backend_memory::InMemoryStore::new(),
    }
}

/// The real server, except every read page claims more remains and returns
/// the cursor it was asked from: a server that never lets a pager advance.
struct StuckCursor(eventstore_bin::Service);

#[tonic::async_trait]
impl EventStoreRpc for StuckCursor {
    async fn append(&self, r: Request<AppendRequest>) -> Result<Response<AppendResponse>, Status> {
        self.0.append(r).await
    }
    async fn read_stream(
        &self,
        r: Request<ReadStreamRequest>,
    ) -> Result<Response<ReadStreamResponse>, Status> {
        let from = r.get_ref().from_aggregate_nonce;
        let mut page = self.0.read_stream(r).await?.into_inner();
        page.is_end = false;
        page.next_from_aggregate_nonce = from;
        Ok(Response::new(page))
    }
    async fn read_all(
        &self,
        r: Request<ReadAllRequest>,
    ) -> Result<Response<ReadAllResponse>, Status> {
        let from = r.get_ref().from_global_nonce;
        let mut page = self.0.read_all(r).await?.into_inner();
        page.is_end = false;
        page.next_from_global_nonce = from;
        Ok(Response::new(page))
    }
    async fn get_server_info(
        &self,
        r: Request<GetServerInfoRequest>,
    ) -> Result<Response<GetServerInfoResponse>, Status> {
        self.0.get_server_info(r).await
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

fn event(aggregate_id: &str, nonce: u64) -> EventData {
    EventData {
        meta: Some(EventMetadata {
            event_id: format!("{aggregate_id}-{nonce}"),
            aggregate_id: aggregate_id.into(),
            aggregate_type: "Order".into(),
            aggregate_nonce: nonce,
            event_type: "OrderNoted".into(),
            event_version: 1,
            content_type: "application/json".into(),
            tenant_id: TENANT.into(),
            ..Default::default()
        }),
        payload: b"{}".to_vec(),
    }
}

/// Append `count` events to `aggregate_id`, one per call.
async fn seed(client: &mut EventStore, aggregate_id: &str, count: u64) {
    for nonce in 1..=count {
        client
            .append(AppendRequest {
                tenant_id: TENANT.into(),
                aggregate_id: aggregate_id.into(),
                aggregate_type: "Order".into(),
                expected_aggregate_nonce: nonce - 1,
                idempotency_key: String::new(),
                events: vec![event(aggregate_id, nonce)],
            })
            .await
            .expect("append");
    }
}

fn nonces(events: &[EventData]) -> Vec<u64> {
    events
        .iter()
        .map(|e| e.meta.as_ref().unwrap().aggregate_nonce)
        .collect()
}

fn stream_request(
    aggregate_id: &str,
    max_count: u32,
    forward: bool,
    from: u64,
) -> ReadStreamRequest {
    ReadStreamRequest {
        tenant_id: TENANT.into(),
        aggregate_id: aggregate_id.into(),
        from_aggregate_nonce: from,
        max_count,
        forward,
    }
}

fn all_request(max_count: u32, forward: bool, from: u64) -> ReadAllRequest {
    ReadAllRequest {
        tenant_id: TENANT.into(),
        from_global_nonce: from,
        max_count,
        forward,
    }
}

#[tokio::test]
async fn stream_pages_read_every_event_including_the_final_page() {
    let (addr, shutdown, handle) = spawn(memory_service()).await;
    let mut client = connect(&addr).await;
    seed(&mut client, "order-1", 5).await;

    // 5 events at 2 per page: the server sets is_end on the page holding
    // event 5, which must still be returned.
    let mut pages = client.stream_pages(stream_request("order-1", 2, true, 1));
    let mut sizes = Vec::new();
    let mut all = Vec::new();
    while let Some(page) = pages.next_page().await.expect("page") {
        sizes.push(page.len());
        all.extend(page);
    }
    assert_eq!(nonces(&all), vec![1, 2, 3, 4, 5]);
    assert_eq!(sizes.iter().sum::<usize>(), 5);
    assert!(pages.next_page().await.expect("exhausted").is_none());

    // A page size that divides the stream exactly.
    let even = client
        .read_stream_to_end(stream_request("order-1", 5, true, 1))
        .await
        .expect("read");
    assert_eq!(nonces(&even), vec![1, 2, 3, 4, 5]);

    // Backward from the head.
    let backward = client
        .read_stream_to_end(stream_request("order-1", 2, false, u64::MAX))
        .await
        .expect("read backward");
    assert_eq!(nonces(&backward), vec![5, 4, 3, 2, 1]);

    // A stream that does not exist is empty, not an error.
    let none = client
        .read_stream_to_end(stream_request("missing", 2, true, 1))
        .await
        .expect("read missing");
    assert!(none.is_empty());

    // An empty page leaves the server's normalized cursor: forward from 0
    // starts at 1, backward ends at 0.
    let mut empty = client.stream_pages(stream_request("missing", 2, true, 0));
    assert!(empty.next_page().await.expect("empty").is_none());
    assert_eq!(empty.cursor(), 1);
    let mut empty = client.stream_pages(stream_request("missing", 2, false, u64::MAX));
    assert!(empty.next_page().await.expect("empty").is_none());
    assert_eq!(empty.cursor(), 0);

    drop(client);
    let _ = shutdown.send(());
    let _ = handle.await;
}

#[tokio::test]
async fn all_pages_read_the_whole_log_and_report_the_tail_cursor() {
    let (addr, shutdown, handle) = spawn(memory_service()).await;
    let mut client = connect(&addr).await;
    seed(&mut client, "order-1", 3).await;
    seed(&mut client, "order-2", 4).await;

    let mut pages = client.all_pages(all_request(3, true, 0));
    let mut seen = 0;
    while let Some(page) = pages.next_page().await.expect("page") {
        assert!(page.len() <= 3);
        seen += page.len();
    }
    assert_eq!(seen, 7);
    let tail = pages.cursor();

    // The cursor is where the next append lands: a tail read from it sees
    // exactly the new event.
    seed(&mut client, "order-3", 1).await;
    let after = client
        .all_pages(all_request(3, true, tail))
        .collect()
        .await
        .expect("tail read");
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].meta.as_ref().unwrap().aggregate_id, "order-3");

    drop(client);
    let _ = shutdown.send(());
    let _ = handle.await;
}

#[tokio::test]
async fn a_cursor_that_does_not_advance_is_an_error_not_a_hang() {
    let (addr, shutdown, handle) = spawn(StuckCursor(memory_service())).await;
    let mut client = connect(&addr).await;
    seed(&mut client, "order-1", 3).await;

    let read = tokio::time::timeout(
        Duration::from_secs(5),
        client.read_stream_to_end(stream_request("order-1", 2, true, 1)),
    )
    .await
    .expect("must not hang");
    let err = read.expect_err("stalled cursor");
    assert_eq!(
        err.downcast_ref::<PagingStalled>(),
        Some(&PagingStalled {
            rpc: "ReadStream",
            forward: true,
            from: 1,
            next: 1,
        })
    );

    let mut pages = client.all_pages(all_request(2, true, 0));
    let err = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Err(err) = pages.next_page().await {
                return err;
            }
        }
    })
    .await
    .expect("must not hang");
    assert!(matches!(
        err.downcast_ref::<PagingStalled>(),
        Some(PagingStalled { rpc: "ReadAll", .. })
    ));
    // A failed pager is finished.
    assert!(pages.next_page().await.expect("finished").is_none());

    drop(client);
    let _ = shutdown.send(());
    let _ = handle.await;
}
