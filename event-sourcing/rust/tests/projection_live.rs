//! Live-server tests for `ProjectionRunner` against the real gRPC event store.

mod common;

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use common::{connect, spawn_server, unique_tenant};
use event_sourcing_rust::client::{
    proto, EventDataStream, EventStoreClient, EventStorePort, ServerInfo,
};
use event_sourcing_rust::error::{Error, Result};
use event_sourcing_rust::event::EventSchema;
use event_sourcing_rust::projection::{
    CheckpointKey, CheckpointStore, CheckpointedProjection, DispatchContext, ExternalCheckpoints,
    InMemoryCheckpointStore, InMemoryProjectionStore, InMemoryTx, LiveProcessor, ProjectionRunner,
    ProjectionStore, RecordedEvent, RunExit, RunnerProgress,
};
use event_sourcing_rust::upcast::Upcasters;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Deposited {
    amount: i64,
}

impl EventSchema for Deposited {
    const EVENT_TYPE: &'static str = "Deposited";
}

/// Append one event; returns its global nonce.
async fn append(
    client: &EventStoreClient,
    tenant: &str,
    aggregate_id: &str,
    nonce: u64,
    event_type: &str,
    amount: i64,
) -> u64 {
    let payload = serde_json::to_vec(&Deposited { amount }).unwrap();
    client
        .append(proto::AppendRequest {
            tenant_id: tenant.into(),
            aggregate_id: aggregate_id.into(),
            aggregate_type: "Account".into(),
            expected_aggregate_nonce: nonce - 1,
            idempotency_key: String::new(),
            events: vec![proto::EventData {
                meta: Some(proto::EventMetadata {
                    event_id: uuid::Uuid::new_v4().to_string(),
                    aggregate_id: aggregate_id.into(),
                    aggregate_type: "Account".into(),
                    aggregate_nonce: nonce,
                    event_type: event_type.into(),
                    event_version: 1,
                    content_type: "application/json".into(),
                    tenant_id: tenant.into(),
                    ..Default::default()
                }),
                payload,
            }],
        })
        .await
        .expect("append")
        .last_global_nonce
}

#[derive(Debug, Clone, Default, PartialEq)]
struct Balances {
    by_account: BTreeMap<String, i64>,
    applied: u64,
}

type MemStore = InMemoryProjectionStore<Balances>;

#[derive(Default, Clone)]
struct Probe {
    calls: Arc<AtomicUsize>,
    contexts: Arc<Mutex<Vec<DispatchContext>>>,
}

struct BalanceProjection {
    name: &'static str,
    version: u32,
    probe: Probe,
    fail_at: Option<u64>,
}

impl BalanceProjection {
    fn new(probe: &Probe) -> Self {
        Self {
            name: "balances",
            version: 1,
            probe: probe.clone(),
            fail_at: None,
        }
    }
}

fn apply(state: &mut Balances, event: &RecordedEvent) -> Result<()> {
    let Deposited { amount } = event.decode()?;
    *state
        .by_account
        .entry(event.aggregate_id.clone())
        .or_default() += amount;
    state.applied += 1;
    Ok(())
}

#[async_trait]
impl CheckpointedProjection<MemStore> for BalanceProjection {
    fn name(&self) -> &str {
        self.name
    }

    fn version(&self) -> u32 {
        self.version
    }

    fn handles(&self, event_type: &str) -> bool {
        event_type == "Deposited"
    }

    async fn handle(
        &mut self,
        tx: &mut InMemoryTx<Balances>,
        event: &RecordedEvent,
        ctx: &DispatchContext,
    ) -> Result<()> {
        self.probe.calls.fetch_add(1, Ordering::SeqCst);
        self.probe.contexts.lock().unwrap().push(*ctx);
        apply(&mut tx.state, event)?;
        if self.fail_at == Some(event.global_nonce) {
            // Fails after mutating the staged state: the store must discard it.
            return Err(Error::domain("handler failure"));
        }
        Ok(())
    }

    async fn reset(&mut self, tx: &mut InMemoryTx<Balances>, _key: &CheckpointKey) -> Result<()> {
        tx.state = Balances::default();
        Ok(())
    }
}

async fn wait_for(
    rx: &mut tokio::sync::watch::Receiver<RunnerProgress>,
    pred: impl Fn(&RunnerProgress) -> bool,
) {
    tokio::time::timeout(Duration::from_secs(10), rx.wait_for(|p| pred(p)))
        .await
        .expect("timed out waiting for runner progress")
        .expect("runner dropped");
}

struct Fixture {
    _server: common::TestServer,
    client: EventStoreClient,
    port: Arc<dyn EventStorePort>,
    tenant: String,
}

async fn fixture() -> Fixture {
    let server = spawn_server().await;
    let client = connect(&server.addr).await;
    Fixture {
        port: Arc::new(client.clone()),
        client,
        _server: server,
        tenant: unique_tenant(),
    }
}

// ---------------------------------------------------------------------------
// Catch-up, live, resume, cancellation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn catch_up_then_live_then_cancel() {
    let f = fixture().await;
    let g1 = append(&f.client, &f.tenant, "a", 1, "Deposited", 10).await;
    append(&f.client, &f.tenant, "a", 2, "Renamed", 0).await; // not handled
    let g3 = append(&f.client, &f.tenant, "b", 1, "Deposited", 5).await;

    let store = Arc::new(MemStore::new());
    let probe = Probe::default();
    let mut runner = ProjectionRunner::new(
        f.port.clone(),
        store.clone(),
        BalanceProjection::new(&probe),
        &f.tenant,
    )
    .with_page_size(2);
    let key = runner.key().clone();
    let mut progress = runner.progress();
    let cancel = CancellationToken::new();
    let task = tokio::spawn({
        let cancel = cancel.clone();
        async move {
            let exit = runner.run(cancel).await;
            (exit, runner)
        }
    });

    wait_for(&mut progress, |p| p.is_live && p.position == g3).await;
    let g4 = append(&f.client, &f.tenant, "a", 3, "Deposited", 1).await;
    wait_for(&mut progress, |p| p.position == g4).await;

    cancel.cancel();
    let (exit, _runner) = task.await.unwrap();
    assert_eq!(exit.unwrap(), RunExit::Cancelled { position: g4 });

    let state = store.state(&key);
    assert_eq!(state.by_account["a"], 11);
    assert_eq!(state.by_account["b"], 5);
    assert_eq!(store.load_checkpoint(&key).await.unwrap(), Some(g4));

    let contexts = probe.contexts.lock().unwrap().clone();
    let flags: Vec<(u64, bool)> = contexts
        .iter()
        .map(|c| (c.global_nonce, c.is_catching_up))
        .collect();
    assert_eq!(flags, vec![(g1, true), (g3, true), (g4, false)]);
    assert!(contexts.iter().all(|c| c.live_boundary_nonce == g3));
}

#[tokio::test]
async fn upcasters_run_before_routing_and_decoding() {
    let f = fixture().await;
    append(&f.client, &f.tenant, "a", 1, "Deposited", 10).await;
    // Old name for the same fact; routed only after the rename.
    let g2 = append(&f.client, &f.tenant, "a", 2, "Credited", 5).await;

    let upcasters = Upcasters::new().rename("Credited", 1, "Deposited", 1, Ok);
    let store = Arc::new(MemStore::new());
    let mut runner = ProjectionRunner::new(
        f.port.clone(),
        store.clone(),
        BalanceProjection::new(&Probe::default()),
        &f.tenant,
    )
    .with_upcasters(upcasters);
    assert_eq!(runner.catch_up().await.unwrap(), g2);
    assert_eq!(store.state(runner.key()).by_account["a"], 15);

    // Without the upcaster the old name is not routed (handles() is false):
    // filtered, never decoded.
    let store = Arc::new(MemStore::new());
    let mut plain = ProjectionRunner::new(
        f.port.clone(),
        store.clone(),
        BalanceProjection::new(&Probe::default()),
        &f.tenant,
    );
    plain.catch_up().await.unwrap();
    assert_eq!(store.state(plain.key()).by_account["a"], 10);
}

#[tokio::test]
async fn failing_upcaster_stops_the_runner_without_advancing() {
    let f = fixture().await;
    let g1 = append(&f.client, &f.tenant, "a", 1, "Deposited", 10).await;
    let g2 = append(&f.client, &f.tenant, "a", 2, "Credited", 5).await;

    let upcasters = Upcasters::new().rename("Credited", 1, "Deposited", 1, |_| {
        Err(Error::domain("cannot migrate"))
    });
    let store = Arc::new(MemStore::new());
    let mut runner = ProjectionRunner::new(
        f.port.clone(),
        store.clone(),
        BalanceProjection::new(&Probe::default()),
        &f.tenant,
    )
    .with_upcasters(upcasters);
    let err = runner.catch_up().await.expect_err("upcaster fails");
    match err {
        Error::ProjectionFailed {
            global_nonce,
            source,
            ..
        } => {
            assert_eq!(global_nonce, g2);
            assert!(matches!(*source, Error::Upcast { .. }), "{source:?}");
        }
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(store.load_checkpoint(runner.key()).await.unwrap(), Some(g1));
}

#[tokio::test]
async fn decode_rejects_unknown_versions() {
    let f = fixture().await;
    append(&f.client, &f.tenant, "a", 1, "Deposited", 1).await;
    let data = f
        .client
        .read_all(proto::ReadAllRequest {
            tenant_id: f.tenant.clone(),
            from_global_nonce: 1,
            max_count: 1,
            forward: true,
        })
        .await
        .unwrap()
        .events
        .remove(0);
    let mut event = RecordedEvent::from_proto(data).unwrap();
    assert_eq!(event.decode::<Deposited>().unwrap().amount, 1);
    event.event_version = 2;
    assert!(matches!(
        event.decode::<Deposited>(),
        Err(Error::UnknownEventVersion { .. })
    ));
    event.event_type = "Other".into();
    assert!(matches!(
        event.decode::<Deposited>(),
        Err(Error::UnknownEventType { .. })
    ));
    event.content_type = "application/x-protobuf".into();
    assert!(matches!(
        event.decode::<Deposited>(),
        Err(Error::UnsupportedContentType { .. })
    ));
}

#[tokio::test]
async fn resumes_from_persisted_checkpoint() {
    let f = fixture().await;
    append(&f.client, &f.tenant, "a", 1, "Deposited", 1).await;
    append(&f.client, &f.tenant, "a", 2, "Deposited", 2).await;
    let store = Arc::new(MemStore::new());

    let probe = Probe::default();
    let mut first = ProjectionRunner::new(
        f.port.clone(),
        store.clone(),
        BalanceProjection::new(&probe),
        &f.tenant,
    );
    first.catch_up().await.unwrap();
    assert_eq!(probe.calls.load(Ordering::SeqCst), 2);

    // A new process: fresh runner and projection, same store.
    let probe2 = Probe::default();
    let mut second = ProjectionRunner::new(
        f.port.clone(),
        store.clone(),
        BalanceProjection::new(&probe2),
        &f.tenant,
    );
    second.catch_up().await.unwrap();
    assert_eq!(
        probe2.calls.load(Ordering::SeqCst),
        0,
        "nothing reprocessed"
    );

    let g3 = append(&f.client, &f.tenant, "a", 3, "Deposited", 4).await;
    assert_eq!(second.catch_up().await.unwrap(), g3);
    assert_eq!(probe2.calls.load(Ordering::SeqCst), 1);
    assert_eq!(store.state(second.key()).by_account["a"], 7);
}

#[tokio::test]
async fn cancellation_before_live_returns_cleanly() {
    let f = fixture().await;
    append(&f.client, &f.tenant, "a", 1, "Deposited", 1).await;
    let store = Arc::new(MemStore::new());
    let mut runner = ProjectionRunner::new(
        f.port.clone(),
        store,
        BalanceProjection::new(&Probe::default()),
        &f.tenant,
    );
    let cancel = CancellationToken::new();
    cancel.cancel();
    let exit = runner.run(cancel).await.unwrap();
    assert_eq!(exit, RunExit::Cancelled { position: 0 });
}

// ---------------------------------------------------------------------------
// Duplicate delivery
// ---------------------------------------------------------------------------

/// Delivers every event twice, on both catch-up and live paths.
struct DuplicatingPort(EventStoreClient);

#[async_trait]
impl EventStorePort for DuplicatingPort {
    async fn append(&self, req: proto::AppendRequest) -> Result<proto::AppendResponse> {
        self.0.append(req).await
    }
    async fn read_stream(
        &self,
        req: proto::ReadStreamRequest,
    ) -> Result<proto::ReadStreamResponse> {
        self.0.read_stream(req).await
    }
    async fn read_all(&self, req: proto::ReadAllRequest) -> Result<proto::ReadAllResponse> {
        let mut resp = self.0.read_all(req.clone()).await?;
        if req.forward {
            resp.events = resp
                .events
                .into_iter()
                .flat_map(|e| [e.clone(), e])
                .collect();
        }
        Ok(resp)
    }
    async fn server_info(&self) -> Result<ServerInfo> {
        self.0.server_info().await
    }
    async fn subscribe(&self, req: proto::SubscribeRequest) -> Result<EventDataStream> {
        use tokio_stream::StreamExt;
        let mut inner = self.0.subscribe(req).await?;
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move {
            while let Some(item) = inner.next().await {
                let items = match item {
                    Ok(e) => vec![Ok(e.clone()), Ok(e)],
                    Err(err) => vec![Err(err)],
                };
                for item in items {
                    if tx.send(item).await.is_err() {
                        return;
                    }
                }
            }
        });
        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
}

#[tokio::test]
async fn duplicate_delivery_is_applied_once() {
    let f = fixture().await;
    append(&f.client, &f.tenant, "a", 1, "Deposited", 1).await;
    let g2 = append(&f.client, &f.tenant, "a", 2, "Deposited", 2).await;

    let port: Arc<dyn EventStorePort> = Arc::new(DuplicatingPort(f.client.clone()));
    let store = Arc::new(MemStore::new());
    let probe = Probe::default();
    let mut runner = ProjectionRunner::new(
        port,
        store.clone(),
        BalanceProjection::new(&probe),
        &f.tenant,
    );
    let key = runner.key().clone();
    let mut progress = runner.progress();
    let cancel = CancellationToken::new();
    let task = tokio::spawn({
        let cancel = cancel.clone();
        async move { runner.run(cancel).await }
    });
    wait_for(&mut progress, |p| p.is_live && p.position == g2).await;
    let g3 = append(&f.client, &f.tenant, "a", 3, "Deposited", 4).await;
    wait_for(&mut progress, |p| p.position == g3).await;
    // Give the duplicate of the live event time to arrive.
    tokio::time::sleep(Duration::from_millis(100)).await;
    cancel.cancel();
    task.await.unwrap().unwrap();

    assert_eq!(probe.calls.load(Ordering::SeqCst), 3);
    let state = store.state(&key);
    assert_eq!(state.applied, 3);
    assert_eq!(state.by_account["a"], 7);
}

// ---------------------------------------------------------------------------
// Failures: handler, subscription stream, restart between apply and checkpoint
// ---------------------------------------------------------------------------

#[tokio::test]
async fn failed_handler_stops_without_advancing_and_resumes() {
    let f = fixture().await;
    let g1 = append(&f.client, &f.tenant, "a", 1, "Deposited", 1).await;
    let g2 = append(&f.client, &f.tenant, "a", 2, "Deposited", 2).await;
    let g3 = append(&f.client, &f.tenant, "a", 3, "Deposited", 4).await;
    let store = Arc::new(MemStore::new());

    let mut failing = BalanceProjection::new(&Probe::default());
    failing.fail_at = Some(g2);
    let mut runner = ProjectionRunner::new(f.port.clone(), store.clone(), failing, &f.tenant);
    let key = runner.key().clone();
    match runner.catch_up().await {
        Err(Error::ProjectionFailed {
            global_nonce,
            projection,
            ..
        }) => {
            assert_eq!(global_nonce, g2);
            assert_eq!(projection, key.to_string());
        }
        other => panic!("expected ProjectionFailed, got {other:?}"),
    }
    assert_eq!(store.load_checkpoint(&key).await.unwrap(), Some(g1));
    assert_eq!(
        store.state(&key).by_account["a"],
        1,
        "failed event rolled back"
    );

    // Restart with a fixed handler: continues at the failed event.
    let probe = Probe::default();
    let mut restarted = ProjectionRunner::new(
        f.port.clone(),
        store.clone(),
        BalanceProjection::new(&probe),
        &f.tenant,
    );
    assert_eq!(restarted.catch_up().await.unwrap(), g3);
    assert_eq!(probe.calls.load(Ordering::SeqCst), 2);
    assert_eq!(store.state(&key).by_account["a"], 7);
    assert_eq!(store.state(&key).applied, 3);
}

enum SubscribeFault {
    Error,
    End,
}

struct FaultySubscribePort(EventStoreClient, SubscribeFault);

#[async_trait]
impl EventStorePort for FaultySubscribePort {
    async fn append(&self, req: proto::AppendRequest) -> Result<proto::AppendResponse> {
        self.0.append(req).await
    }
    async fn read_stream(
        &self,
        req: proto::ReadStreamRequest,
    ) -> Result<proto::ReadStreamResponse> {
        self.0.read_stream(req).await
    }
    async fn read_all(&self, req: proto::ReadAllRequest) -> Result<proto::ReadAllResponse> {
        self.0.read_all(req).await
    }
    async fn server_info(&self) -> Result<ServerInfo> {
        self.0.server_info().await
    }
    async fn subscribe(&self, _req: proto::SubscribeRequest) -> Result<EventDataStream> {
        let items: Vec<Result<proto::EventData>> = match self.1 {
            SubscribeFault::Error => vec![Err(Error::from(tonic::Status::internal(
                "backend query failed",
            )))],
            SubscribeFault::End => vec![],
        };
        Ok(Box::pin(tokio_stream::iter(items)))
    }
}

#[tokio::test]
async fn subscription_errors_and_end_of_stream_propagate() {
    let f = fixture().await;
    let g1 = append(&f.client, &f.tenant, "a", 1, "Deposited", 1).await;

    for (fault, code) in [
        (SubscribeFault::Error, tonic::Code::Internal),
        (SubscribeFault::End, tonic::Code::Unavailable),
    ] {
        let port: Arc<dyn EventStorePort> = Arc::new(FaultySubscribePort(f.client.clone(), fault));
        let store = Arc::new(MemStore::new());
        let mut runner = ProjectionRunner::new(
            port,
            store.clone(),
            BalanceProjection::new(&Probe::default()),
            &f.tenant,
        );
        let err = runner
            .run(CancellationToken::new())
            .await
            .expect_err("stream failure must surface");
        assert_eq!(err.status_code(), Some(code), "{err:?}");
        // Catch-up work before the failure stays committed.
        assert_eq!(store.load_checkpoint(runner.key()).await.unwrap(), Some(g1));
    }
}

/// Transactional store whose commit fails once at a given position, as if
/// the process died after handling but before the checkpoint committed.
struct CrashOnCommit {
    inner: MemStore,
    crash_at: AtomicU64,
}

#[async_trait]
impl ProjectionStore for CrashOnCommit {
    type Tx = InMemoryTx<Balances>;
    async fn load_checkpoint(&self, key: &CheckpointKey) -> Result<Option<u64>> {
        self.inner.load_checkpoint(key).await
    }
    async fn begin(&self, key: &CheckpointKey) -> Result<Self::Tx> {
        self.inner.begin(key).await
    }
    async fn commit(&self, tx: Self::Tx, key: &CheckpointKey, position: u64) -> Result<()> {
        if self
            .crash_at
            .compare_exchange(position, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return Err(Error::from(tonic::Status::unavailable("crash")));
        }
        self.inner.commit(tx, key, position).await
    }
    async fn delete_checkpoint(&self, key: &CheckpointKey) -> Result<()> {
        self.inner.delete_checkpoint(key).await
    }
    async fn begin_reset(&self, key: &CheckpointKey) -> Result<Self::Tx> {
        self.inner.begin_reset(key).await
    }
    async fn commit_reset(&self, tx: Self::Tx, key: &CheckpointKey) -> Result<()> {
        self.inner.commit_reset(tx, key).await
    }
}

#[async_trait]
impl CheckpointedProjection<CrashOnCommit> for BalanceProjection {
    fn name(&self) -> &str {
        self.name
    }
    fn version(&self) -> u32 {
        self.version
    }
    async fn handle(
        &mut self,
        tx: &mut InMemoryTx<Balances>,
        event: &RecordedEvent,
        _ctx: &DispatchContext,
    ) -> Result<()> {
        self.probe.calls.fetch_add(1, Ordering::SeqCst);
        apply(&mut tx.state, event)
    }
    async fn reset(&mut self, tx: &mut InMemoryTx<Balances>, _key: &CheckpointKey) -> Result<()> {
        tx.state = Balances::default();
        Ok(())
    }
}

#[tokio::test]
async fn restart_between_processing_and_checkpoint_transactional() {
    let f = fixture().await;
    append(&f.client, &f.tenant, "a", 1, "Deposited", 1).await;
    let g2 = append(&f.client, &f.tenant, "a", 2, "Deposited", 2).await;
    let store = Arc::new(CrashOnCommit {
        inner: MemStore::new(),
        crash_at: AtomicU64::new(g2),
    });
    let probe = Probe::default();

    let mut runner = ProjectionRunner::new(
        f.port.clone(),
        store.clone(),
        BalanceProjection::new(&probe),
        &f.tenant,
    );
    let key = runner.key().clone();
    runner.catch_up().await.expect_err("crash at commit");
    // Atomic: the handled-but-uncommitted event left no trace.
    assert_eq!(store.inner.state(&key).applied, 1);

    let mut restarted = ProjectionRunner::new(
        f.port.clone(),
        store.clone(),
        BalanceProjection::new(&probe),
        &f.tenant,
    );
    restarted.catch_up().await.unwrap();
    assert_eq!(probe.calls.load(Ordering::SeqCst), 3, "event 2 redelivered");
    let state = store.inner.state(&key);
    assert_eq!(state.applied, 2, "applied exactly once");
    assert_eq!(state.by_account["a"], 3);
}

/// Checkpoint store that fails one save, like a crash after the external
/// write and before the checkpoint.
struct CrashOnSave {
    inner: InMemoryCheckpointStore,
    crash_at: AtomicU64,
}

#[async_trait]
impl CheckpointStore for CrashOnSave {
    async fn load(&self, key: &CheckpointKey) -> Result<Option<u64>> {
        self.inner.load(key).await
    }
    async fn save(&self, key: &CheckpointKey, position: u64) -> Result<()> {
        if self
            .crash_at
            .compare_exchange(position, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return Err(Error::from(tonic::Status::unavailable("crash")));
        }
        self.inner.save(key, position).await
    }
    async fn delete(&self, key: &CheckpointKey) -> Result<()> {
        self.inner.delete(key).await
    }
}

/// External "search index": an idempotent upsert keyed by event id.
#[derive(Default, Clone)]
struct ExternalIndex {
    docs: Arc<Mutex<HashMap<String, i64>>>,
    writes: Arc<AtomicUsize>,
}

struct IndexProjection {
    index: ExternalIndex,
}

type ExtStore = ExternalCheckpoints<CrashOnSave>;

#[async_trait]
impl CheckpointedProjection<ExtStore> for IndexProjection {
    fn name(&self) -> &str {
        "index"
    }
    fn version(&self) -> u32 {
        1
    }
    async fn handle(
        &mut self,
        _tx: &mut (),
        event: &RecordedEvent,
        _ctx: &DispatchContext,
    ) -> Result<()> {
        let Deposited { amount } = event.decode()?;
        self.index.writes.fetch_add(1, Ordering::SeqCst);
        self.index
            .docs
            .lock()
            .unwrap()
            .insert(event.event_id.clone(), amount);
        Ok(())
    }
    async fn reset(&mut self, _tx: &mut (), _key: &CheckpointKey) -> Result<()> {
        self.index.docs.lock().unwrap().clear();
        Ok(())
    }
}

#[tokio::test]
async fn restart_between_processing_and_checkpoint_external_is_idempotent() {
    let f = fixture().await;
    let g1 = append(&f.client, &f.tenant, "a", 1, "Deposited", 1).await;
    let g2 = append(&f.client, &f.tenant, "a", 2, "Deposited", 2).await;
    let g3 = append(&f.client, &f.tenant, "a", 3, "Deposited", 4).await;
    let store = Arc::new(ExternalCheckpoints::new(CrashOnSave {
        inner: InMemoryCheckpointStore::new(),
        crash_at: AtomicU64::new(g2),
    }));
    let index = ExternalIndex::default();

    let mut runner = ProjectionRunner::new(
        f.port.clone(),
        store.clone(),
        IndexProjection {
            index: index.clone(),
        },
        &f.tenant,
    );
    let key = runner.key().clone();
    runner
        .catch_up()
        .await
        .expect_err("crash before checkpoint");
    assert_eq!(
        index.docs.lock().unwrap().len(),
        2,
        "external write happened"
    );
    assert_eq!(store.load_checkpoint(&key).await.unwrap(), Some(g1));

    let mut restarted = ProjectionRunner::new(
        f.port.clone(),
        store.clone(),
        IndexProjection {
            index: index.clone(),
        },
        &f.tenant,
    );
    assert_eq!(restarted.catch_up().await.unwrap(), g3);
    assert_eq!(
        index.writes.load(Ordering::SeqCst),
        4,
        "event 2 redelivered"
    );
    let docs = index.docs.lock().unwrap();
    assert_eq!(docs.len(), 3, "idempotent upsert: no duplicate documents");
    assert_eq!(docs.values().sum::<i64>(), 7);
}

// ---------------------------------------------------------------------------
// Independent positions, rebuilds
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tenants_and_projections_have_independent_positions() {
    let f = fixture().await;
    let tenant_b = unique_tenant();
    append(&f.client, &f.tenant, "a", 1, "Deposited", 1).await;
    let a2 = append(&f.client, &f.tenant, "a", 2, "Deposited", 2).await;
    let b1 = append(&f.client, &tenant_b, "a", 1, "Deposited", 100).await;
    let store = Arc::new(MemStore::new());

    let mut a_balances = ProjectionRunner::new(
        f.port.clone(),
        store.clone(),
        BalanceProjection::new(&Probe::default()),
        &f.tenant,
    );
    let mut b_balances = ProjectionRunner::new(
        f.port.clone(),
        store.clone(),
        BalanceProjection::new(&Probe::default()),
        &tenant_b,
    );
    let mut other = BalanceProjection::new(&Probe::default());
    other.name = "balances-copy";
    let mut a_other = ProjectionRunner::new(f.port.clone(), store.clone(), other, &f.tenant);

    assert_eq!(a_balances.catch_up().await.unwrap(), a2);
    assert_eq!(b_balances.catch_up().await.unwrap(), b1);
    assert_eq!(store.load_checkpoint(a_other.key()).await.unwrap(), None);

    assert_eq!(store.state(a_balances.key()).by_account["a"], 3);
    assert_eq!(store.state(b_balances.key()).by_account["a"], 100);

    // Advancing one key leaves the others alone.
    a_other.catch_up().await.unwrap();
    let a3 = append(&f.client, &f.tenant, "a", 3, "Deposited", 4).await;
    a_balances.catch_up().await.unwrap();
    assert_eq!(
        store.load_checkpoint(a_balances.key()).await.unwrap(),
        Some(a3)
    );
    assert_eq!(
        store.load_checkpoint(a_other.key()).await.unwrap(),
        Some(a2)
    );
    assert_eq!(
        store.load_checkpoint(b_balances.key()).await.unwrap(),
        Some(b1)
    );
}

#[tokio::test]
async fn feed_prefix_is_part_of_identity() {
    let f = fixture().await;
    append(&f.client, &f.tenant, "order-1", 1, "Deposited", 1).await;
    append(&f.client, &f.tenant, "user-1", 1, "Deposited", 10).await;
    let store = Arc::new(MemStore::new());
    let mut orders = ProjectionRunner::new(
        f.port.clone(),
        store.clone(),
        BalanceProjection::new(&Probe::default()),
        &f.tenant,
    )
    .with_feed_prefix("order-");
    let mut all = ProjectionRunner::new(
        f.port.clone(),
        store.clone(),
        BalanceProjection::new(&Probe::default()),
        &f.tenant,
    );
    orders.catch_up().await.unwrap();
    all.catch_up().await.unwrap();
    assert_ne!(orders.key(), all.key());
    assert_eq!(store.state(orders.key()).applied, 1);
    assert_eq!(store.state(all.key()).applied, 2);
}

#[tokio::test]
async fn rebuild_is_equivalent_and_versions_build_independently() {
    let f = fixture().await;
    for (i, amount) in [3, 5, 7, 11].into_iter().enumerate() {
        let id = if i % 2 == 0 { "a" } else { "b" };
        append(
            &f.client,
            &f.tenant,
            id,
            (i / 2 + 1) as u64,
            "Deposited",
            amount,
        )
        .await;
    }
    let store = Arc::new(MemStore::new());
    let mut v1 = ProjectionRunner::new(
        f.port.clone(),
        store.clone(),
        BalanceProjection::new(&Probe::default()),
        &f.tenant,
    )
    .with_page_size(3);
    let head = v1.catch_up().await.unwrap();
    let built = store.state(v1.key());

    v1.rebuild().await.unwrap();
    assert_eq!(store.state(v1.key()), Balances::default());
    assert_eq!(store.load_checkpoint(v1.key()).await.unwrap(), None);
    assert_eq!(v1.catch_up().await.unwrap(), head);
    assert_eq!(store.state(v1.key()), built, "rebuild equivalence");

    // Version 2 builds from zero next to version 1, which is untouched.
    let mut projection_v2 = BalanceProjection::new(&Probe::default());
    projection_v2.version = 2;
    let mut v2 = ProjectionRunner::new(f.port.clone(), store.clone(), projection_v2, &f.tenant);
    assert_eq!(store.load_checkpoint(v2.key()).await.unwrap(), None);
    v2.catch_up().await.unwrap();
    assert_eq!(store.state(v2.key()), built);
    v2.rebuild().await.unwrap();
    assert_eq!(store.load_checkpoint(v1.key()).await.unwrap(), Some(head));
    assert_eq!(store.state(v1.key()), built, "v2 rebuild leaves v1 alone");
}

// ---------------------------------------------------------------------------
// Process manager: side effects only for live events
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq)]
struct Todos {
    pending: Vec<String>,
}

type TodoStore = InMemoryProjectionStore<Todos>;

struct TodoProjection;

#[async_trait]
impl CheckpointedProjection<TodoStore> for TodoProjection {
    fn name(&self) -> &str {
        "notify-todos"
    }
    fn version(&self) -> u32 {
        1
    }
    async fn handle(
        &mut self,
        tx: &mut InMemoryTx<Todos>,
        event: &RecordedEvent,
        _ctx: &DispatchContext,
    ) -> Result<()> {
        // Pure: record what needs doing, do nothing.
        tx.state.pending.push(event.event_id.clone());
        Ok(())
    }
    async fn reset(&mut self, tx: &mut InMemoryTx<Todos>, _key: &CheckpointKey) -> Result<()> {
        tx.state = Todos::default();
        Ok(())
    }
}

struct Notifier {
    passes: AtomicUsize,
    fail_passes: AtomicU32,
    sent: Mutex<Vec<String>>,
    store: Arc<TodoStore>,
    key: CheckpointKey,
}

impl Notifier {
    fn new(store: &Arc<TodoStore>, key: &CheckpointKey, fail_passes: u32) -> Arc<Self> {
        Arc::new(Self {
            passes: AtomicUsize::new(0),
            fail_passes: AtomicU32::new(fail_passes),
            sent: Mutex::new(vec![]),
            store: store.clone(),
            key: key.clone(),
        })
    }

    fn sent(&self) -> usize {
        self.sent.lock().unwrap().len()
    }
}

async fn eventually(what: &str, cond: impl Fn() -> bool) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !cond() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out: {what}"));
}

#[async_trait]
impl LiveProcessor for Notifier {
    async fn process_pending(&self) -> Result<usize> {
        self.passes.fetch_add(1, Ordering::SeqCst);
        if common::take(&self.fail_passes) {
            return Err(Error::from(tonic::Status::unavailable("smtp down")));
        }
        let todos = self.store.state(&self.key);
        let mut sent = self.sent.lock().unwrap();
        let mut n = 0;
        for id in todos.pending {
            if !sent.contains(&id) {
                sent.push(id); // idempotent "side effect"
                n += 1;
            }
        }
        Ok(n)
    }
}

#[tokio::test]
async fn live_processor_never_runs_during_replay() {
    let f = fixture().await;
    let mut last = 0;
    for nonce in 1..=5 {
        last = append(&f.client, &f.tenant, "a", nonce, "Deposited", 1).await;
    }
    let store = Arc::new(TodoStore::new());
    let mut runner =
        ProjectionRunner::new(f.port.clone(), store.clone(), TodoProjection, &f.tenant);
    let notifier = Notifier::new(&store, runner.key(), 0);
    runner = runner
        .with_live_processor(notifier.clone())
        .drain_pending_on_live_start(false);
    let mut progress = runner.progress();
    let cancel = CancellationToken::new();
    let task = tokio::spawn({
        let cancel = cancel.clone();
        async move { runner.run(cancel).await }
    });

    wait_for(&mut progress, |p| p.is_live && p.position == last).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        notifier.passes.load(Ordering::SeqCst),
        0,
        "no side effects in replay"
    );

    append(&f.client, &f.tenant, "a", 6, "Deposited", 1).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while notifier.passes.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("processor woken by live event");
    cancel.cancel();
    task.await.unwrap().unwrap();
    assert_eq!(notifier.passes.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn failed_processor_pass_is_retried_without_new_events() {
    let f = fixture().await;
    let store = Arc::new(TodoStore::new());
    let mut runner =
        ProjectionRunner::new(f.port.clone(), store.clone(), TodoProjection, &f.tenant);
    let notifier = Notifier::new(&store, runner.key(), 2);
    runner = runner
        .with_live_processor(notifier.clone())
        .with_processor_retry_delay(Duration::from_millis(20))
        .drain_pending_on_live_start(false);
    let mut progress = runner.progress();
    let cancel = CancellationToken::new();
    let task = tokio::spawn({
        let cancel = cancel.clone();
        async move { runner.run(cancel).await }
    });
    wait_for(&mut progress, |p| p.is_live).await;

    append(&f.client, &f.tenant, "a", 1, "Deposited", 1).await;
    eventually("pending item eventually processed", || notifier.sent() == 1).await;
    assert_eq!(
        notifier.passes.load(Ordering::SeqCst),
        3,
        "2 failures + 1 success"
    );
    cancel.cancel();
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn drain_on_live_start_resumes_stranded_items_after_replay() {
    let f = fixture().await;
    let mut last = 0;
    for nonce in 1..=3 {
        last = append(&f.client, &f.tenant, "a", nonce, "Deposited", 1).await;
    }
    let store = Arc::new(TodoStore::new());
    let mut runner =
        ProjectionRunner::new(f.port.clone(), store.clone(), TodoProjection, &f.tenant);
    let notifier = Notifier::new(&store, runner.key(), 0);
    runner = runner
        .with_live_processor(notifier.clone())
        .drain_pending_on_live_start(true);
    let mut progress = runner.progress();
    let cancel = CancellationToken::new();
    let task = tokio::spawn({
        let cancel = cancel.clone();
        async move { runner.run(cancel).await }
    });
    wait_for(&mut progress, |p| p.is_live && p.position == last).await;
    // One pass, after catch-up committed everything (never during replay).
    eventually("startup drain", || notifier.sent() == 3).await;
    assert_eq!(notifier.passes.load(Ordering::SeqCst), 1);
    cancel.cancel();
    task.await.unwrap().unwrap();
}

// ---------------------------------------------------------------------------
// Ordering, task lifetime, prefix validation
// ---------------------------------------------------------------------------

/// Swaps the first two live events, like a backend that broadcasts out of
/// commit order.
struct ReorderingPort(EventStoreClient);

#[async_trait]
impl EventStorePort for ReorderingPort {
    async fn append(&self, req: proto::AppendRequest) -> Result<proto::AppendResponse> {
        self.0.append(req).await
    }
    async fn read_stream(
        &self,
        req: proto::ReadStreamRequest,
    ) -> Result<proto::ReadStreamResponse> {
        self.0.read_stream(req).await
    }
    async fn read_all(&self, req: proto::ReadAllRequest) -> Result<proto::ReadAllResponse> {
        self.0.read_all(req).await
    }
    async fn server_info(&self) -> Result<ServerInfo> {
        self.0.server_info().await
    }
    async fn subscribe(&self, req: proto::SubscribeRequest) -> Result<EventDataStream> {
        use tokio_stream::StreamExt;
        let mut inner = self.0.subscribe(req).await?;
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move {
            let Some(first) = inner.next().await else {
                return;
            };
            let Some(second) = inner.next().await else {
                return;
            };
            for item in [second, first] {
                if tx.send(item).await.is_err() {
                    return;
                }
            }
            while let Some(item) = inner.next().await {
                if tx.send(item).await.is_err() {
                    return;
                }
            }
        });
        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
}

#[tokio::test]
async fn out_of_order_live_delivery_fails_loudly() {
    let f = fixture().await;
    let g1 = append(&f.client, &f.tenant, "a", 1, "Deposited", 1).await;
    let port: Arc<dyn EventStorePort> = Arc::new(ReorderingPort(f.client.clone()));
    let store = Arc::new(MemStore::new());
    let mut runner = ProjectionRunner::new(
        port,
        store.clone(),
        BalanceProjection::new(&Probe::default()),
        &f.tenant,
    );
    let mut progress = runner.progress();
    let task = tokio::spawn(async move { runner.run(CancellationToken::new()).await });
    wait_for(&mut progress, |p| p.is_live && p.position == g1).await;

    let g2 = append(&f.client, &f.tenant, "a", 2, "Deposited", 2).await;
    let g3 = append(&f.client, &f.tenant, "b", 1, "Deposited", 4).await;
    let err = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("runner must stop")
        .unwrap()
        .expect_err("reordered live event must not be skipped silently");
    match err {
        Error::OutOfOrderDelivery {
            last_applied,
            received,
            ..
        } => assert_eq!((last_applied, received), (g3, g2)),
        other => panic!("expected OutOfOrderDelivery, got {other:?}"),
    }
}

#[tokio::test]
async fn dropping_run_stops_the_live_processor() {
    let f = fixture().await;
    let store = Arc::new(TodoStore::new());
    let mut runner =
        ProjectionRunner::new(f.port.clone(), store.clone(), TodoProjection, &f.tenant);
    // Fails forever and retries every 5 ms: keeps calling while alive.
    let notifier = Notifier::new(&store, runner.key(), u32::MAX);
    runner = runner
        .with_live_processor(notifier.clone())
        .with_processor_retry_delay(Duration::from_millis(5));
    let mut progress = runner.progress();
    let task = tokio::spawn(async move { runner.run(CancellationToken::new()).await });
    wait_for(&mut progress, |p| p.is_live).await;
    eventually("processor retrying", || {
        notifier.passes.load(Ordering::SeqCst) >= 2
    })
    .await;

    task.abort();
    let _ = task.await;
    tokio::time::sleep(Duration::from_millis(30)).await;
    let after_abort = notifier.passes.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        notifier.passes.load(Ordering::SeqCst),
        after_abort,
        "processor task outlived its runner"
    );
}

/// `\`, `%` and `_` in a feed prefix are ordinary characters (#361): the
/// runner accepts them, and catch-up and live both deliver exactly the
/// feed's aggregates. `foreign` would match an unescaped SQL LIKE pattern;
/// for `acct\` an unescaped LIKE would instead miss the feed's own events.
#[tokio::test]
async fn like_wildcard_characters_in_feed_prefix_match_literally() {
    for (prefix, foreign) in [
        (r"acct\", "acct%x"),
        ("acct%", "acct-x"),
        ("acct_", "acctXx"),
    ] {
        let f = fixture().await;
        let own = format!("{prefix}1");
        append(&f.client, &f.tenant, &own, 1, "Deposited", 1).await;
        append(&f.client, &f.tenant, foreign, 1, "Deposited", 100).await;

        let store = Arc::new(MemStore::new());
        let mut runner = ProjectionRunner::new(
            f.port.clone(),
            store.clone(),
            BalanceProjection::new(&Probe::default()),
            &f.tenant,
        )
        .with_feed_prefix(prefix);
        let key = runner.key().clone();
        let mut progress = runner.progress();
        let cancel = CancellationToken::new();
        let task = tokio::spawn({
            let cancel = cancel.clone();
            async move { runner.run(cancel).await }
        });
        wait_for(&mut progress, |p| p.is_live).await;

        append(&f.client, &f.tenant, foreign, 2, "Deposited", 100).await;
        let last = append(&f.client, &f.tenant, &own, 2, "Deposited", 2).await;
        wait_for(&mut progress, |p| p.position == last).await;
        cancel.cancel();
        task.await.unwrap().expect("runner accepts the prefix");

        let state = store.state(&key);
        assert_eq!(
            state.by_account,
            BTreeMap::from([(own.clone(), 3)]),
            "prefix '{prefix}'"
        );
        assert_eq!(state.applied, 2, "prefix '{prefix}'");
    }
}

/// Blocks inside `process_pending` until released.
#[derive(Default)]
struct BlockingProcessor {
    started: AtomicUsize,
    completed: AtomicUsize,
    gate: tokio::sync::Notify,
}

#[async_trait]
impl LiveProcessor for BlockingProcessor {
    async fn process_pending(&self) -> Result<usize> {
        self.started.fetch_add(1, Ordering::SeqCst);
        self.gate.notified().await;
        self.completed.fetch_add(1, Ordering::SeqCst);
        Ok(1)
    }
}

#[tokio::test]
async fn aborting_run_during_graceful_shutdown_still_stops_the_processor() {
    let f = fixture().await;
    let store = Arc::new(TodoStore::new());
    let processor = Arc::new(BlockingProcessor::default());
    // The startup drain starts a pass that blocks.
    let mut runner = ProjectionRunner::new(f.port.clone(), store, TodoProjection, &f.tenant)
        .with_live_processor(processor.clone());
    let mut progress = runner.progress();
    let cancel = CancellationToken::new();
    let task = tokio::spawn({
        let cancel = cancel.clone();
        async move { runner.run(cancel).await }
    });
    wait_for(&mut progress, |p| p.is_live).await;
    eventually("pass started", || {
        processor.started.load(Ordering::SeqCst) == 1
    })
    .await;

    // Graceful shutdown waits for the blocked pass; abort run() meanwhile.
    cancel.cancel();
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert!(
        !task.is_finished(),
        "run() is waiting for the in-flight pass"
    );
    task.abort();
    let _ = task.await;

    // Releasing the gate must not let the aborted pass finish its work.
    processor.gate.notify_waiters();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        processor.completed.load(Ordering::SeqCst),
        0,
        "processor pass outlived an aborted runner"
    );
}
