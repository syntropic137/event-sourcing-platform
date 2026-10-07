//! `ProjectionRunner::run_supervised` against the real gRPC event store:
//! reconnects (fault-injecting port and TCP proxy), DATA_LOSS halts
//! (ADR-026), processor panics, the capability guard, and cancellation.

mod common;

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use common::proxy::FaultProxy;
use common::{connect, spawn_server, take, unique_tenant};
use event_sourcing_rust::client::{
    capabilities, proto, EventDataStream, EventStoreClient, EventStorePort, ServerInfo,
};
use event_sourcing_rust::error::{Error, Result, UNDECODABLE_GLOBAL_NONCE_KEY};
use event_sourcing_rust::projection::{
    BackoffPolicy, CheckpointKey, CheckpointedProjection, DispatchContext, InMemoryProjectionStore,
    InMemoryTx, LiveProcessor, ProcessorPanicPolicy, ProjectionRunner, ProjectionStore,
    RecordedEvent, RunExit, RunnerHealth, RunnerState,
};
use tokio::sync::watch;
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

async fn append(client: &EventStoreClient, tenant: &str, aggregate_id: &str, nonce: u64) -> u64 {
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
                    event_type: "Deposited".into(),
                    event_version: 1,
                    content_type: "application/json".into(),
                    tenant_id: tenant.into(),
                    ..Default::default()
                }),
                payload: br#"{"amount":1}"#.to_vec(),
            }],
        })
        .await
        .expect("append")
        .last_global_nonce
}

/// Read model: every applied position, and one to-do per event (ADR-025:
/// to-dos are written for every event, executed only while live).
#[derive(Debug, Clone, Default, PartialEq)]
struct Ledger {
    applied: Vec<u64>,
    todos: Vec<String>,
}

type Store = InMemoryProjectionStore<Ledger>;

#[derive(Default)]
struct LedgerProjection {
    handled: Arc<AtomicUsize>,
    fail_at: Option<u64>,
}

#[async_trait]
impl CheckpointedProjection<Store> for LedgerProjection {
    fn name(&self) -> &str {
        "ledger"
    }
    fn version(&self) -> u32 {
        1
    }
    async fn handle(
        &mut self,
        tx: &mut InMemoryTx<Ledger>,
        event: &RecordedEvent,
        _ctx: &DispatchContext,
    ) -> Result<()> {
        self.handled.fetch_add(1, Ordering::SeqCst);
        if self.fail_at == Some(event.global_nonce) {
            return Err(Error::domain("handler failure"));
        }
        tx.state.applied.push(event.global_nonce);
        tx.state.todos.push(event.event_id.clone());
        Ok(())
    }
    async fn reset(&mut self, tx: &mut InMemoryTx<Ledger>, _key: &CheckpointKey) -> Result<()> {
        tx.state = Ledger::default();
        Ok(())
    }
}

/// Executes to-dos with durable dedup (the `done` set), like ADR-025
/// requires. `effects` counts every executed side effect per to-do.
struct Mailer {
    store: Arc<Store>,
    key: CheckpointKey,
    done: Mutex<HashSet<String>>,
    effects: AtomicUsize,
    passes: AtomicUsize,
    panic_passes: AtomicU32,
}

impl Mailer {
    fn new(store: &Arc<Store>, key: &CheckpointKey, panic_passes: u32) -> Arc<Self> {
        Arc::new(Self {
            store: store.clone(),
            key: key.clone(),
            done: Mutex::new(HashSet::new()),
            effects: AtomicUsize::new(0),
            passes: AtomicUsize::new(0),
            panic_passes: AtomicU32::new(panic_passes),
        })
    }

    fn done(&self) -> HashSet<String> {
        self.done.lock().unwrap().clone()
    }
}

#[async_trait]
impl LiveProcessor for Mailer {
    async fn process_pending(&self) -> Result<usize> {
        self.passes.fetch_add(1, Ordering::SeqCst);
        if take(&self.panic_passes) {
            panic!("mailer exploded");
        }
        let todos = self.store.state(&self.key).todos;
        let mut done = self.done.lock().unwrap();
        let mut n = 0;
        for id in todos {
            if done.insert(id) {
                self.effects.fetch_add(1, Ordering::SeqCst);
                n += 1;
            }
        }
        Ok(n)
    }
}

fn fast() -> BackoffPolicy {
    BackoffPolicy::new(Duration::from_millis(10), Duration::from_millis(50))
}

async fn wait_health(rx: &mut watch::Receiver<RunnerHealth>, pred: impl Fn(&RunnerHealth) -> bool) {
    let waited = tokio::time::timeout(Duration::from_secs(10), async {
        rx.wait_for(|h| pred(h)).await.map(|_| ())
    })
    .await;
    match waited {
        Ok(result) => result.expect("runner dropped"),
        Err(_) => panic!("timed out waiting for health; last: {:?}", *rx.borrow()),
    }
}

async fn eventually(what: &str, cond: impl Fn() -> bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !cond() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out: {what}"));
}

fn data_loss(global_nonce: u64) -> Error {
    let mut metadata = tonic::metadata::MetadataMap::new();
    metadata.insert(UNDECODABLE_GLOBAL_NONCE_KEY, global_nonce.into());
    Error::from(tonic::Status::with_metadata(
        tonic::Code::DataLoss,
        format!("stored event at global_nonce {global_nonce} cannot be decoded"),
        metadata,
    ))
}

fn nonce_of(e: &proto::EventData) -> u64 {
    e.meta.as_ref().map(|m| m.global_nonce).unwrap_or(0)
}

/// Fault-injecting port around a real client.
#[derive(Default)]
struct Faults {
    /// 1-based index of the forward `read_all` call that fails once with
    /// `UNAVAILABLE` (0 = none).
    unavailable_page: AtomicU32,
    /// Every `read_all` fails with `UNAVAILABLE`.
    always_unavailable: AtomicBool,
    /// The next subscription fails with `UNAVAILABLE` after delivering this
    /// many events (`u32::MAX` = never).
    live_unavailable_after: AtomicU32,
    /// Position of an "undecodable" event (0 = none): reads that would
    /// return it and subscriptions reaching it fail with `DATA_LOSS`, as the
    /// Postgres backend does (ADR-026).
    bad: AtomicU64,
    /// When non-zero and `bad` is unset, the first live event above this
    /// position becomes `bad` (positions are not predictable on Postgres,
    /// whose global nonce is shared by every tenant).
    bad_after: AtomicU64,
    /// `server_info` never answers (a stalled server).
    stall_server_info: AtomicBool,
    /// Replaces the server's info when set.
    server_info: Mutex<Option<ServerInfo>>,
    forward_reads: AtomicU32,
    read_alls: AtomicU32,
    subscribes: AtomicU32,
    server_infos: AtomicU32,
}

struct FaultPort {
    inner: EventStoreClient,
    faults: Arc<Faults>,
}

impl FaultPort {
    fn wrap(inner: &EventStoreClient) -> (Arc<dyn EventStorePort>, Arc<Faults>) {
        let faults = Arc::new(Faults {
            live_unavailable_after: AtomicU32::new(u32::MAX),
            ..Faults::default()
        });
        let port = Arc::new(FaultPort {
            inner: inner.clone(),
            faults: faults.clone(),
        });
        (port, faults)
    }
}

#[async_trait]
impl EventStorePort for FaultPort {
    async fn append(&self, req: proto::AppendRequest) -> Result<proto::AppendResponse> {
        self.inner.append(req).await
    }

    async fn read_stream(
        &self,
        req: proto::ReadStreamRequest,
    ) -> Result<proto::ReadStreamResponse> {
        self.inner.read_stream(req).await
    }

    async fn read_all(&self, req: proto::ReadAllRequest) -> Result<proto::ReadAllResponse> {
        let f = &self.faults;
        f.read_alls.fetch_add(1, Ordering::SeqCst);
        if f.always_unavailable.load(Ordering::SeqCst) {
            return Err(Error::from(tonic::Status::unavailable("store down")));
        }
        if req.forward {
            let n = f.forward_reads.fetch_add(1, Ordering::SeqCst) + 1;
            if n == f.unavailable_page.load(Ordering::SeqCst) {
                return Err(Error::from(tonic::Status::unavailable(
                    "replay query failed; resume from global_nonce N",
                )));
            }
        }
        let resp = self.inner.read_all(req).await?;
        let bad = f.bad.load(Ordering::SeqCst);
        if bad != 0 && resp.events.iter().any(|e| nonce_of(e) == bad) {
            return Err(data_loss(bad));
        }
        Ok(resp)
    }

    async fn subscribe(&self, req: proto::SubscribeRequest) -> Result<EventDataStream> {
        let faults = self.faults.clone();
        faults.subscribes.fetch_add(1, Ordering::SeqCst);
        let fail_after = faults
            .live_unavailable_after
            .swap(u32::MAX, Ordering::SeqCst);
        let mut inner = self.inner.subscribe(req).await?;
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move {
            let mut delivered = 0u32;
            while let Some(item) = inner.next().await {
                let item = match item {
                    Ok(e) => {
                        let after = faults.bad_after.load(Ordering::SeqCst);
                        if after != 0 && nonce_of(&e) > after {
                            let _ = faults.bad.compare_exchange(
                                0,
                                nonce_of(&e),
                                Ordering::SeqCst,
                                Ordering::SeqCst,
                            );
                        }
                        let bad = faults.bad.load(Ordering::SeqCst);
                        if bad != 0 && nonce_of(&e) == bad {
                            let _ = tx.send(Err(data_loss(bad))).await;
                            return;
                        }
                        if delivered == fail_after {
                            let _ = tx
                                .send(Err(Error::from(tonic::Status::unavailable(
                                    "live query failed",
                                ))))
                                .await;
                            return;
                        }
                        delivered += 1;
                        Ok(e)
                    }
                    Err(err) => Err(err),
                };
                if tx.send(item).await.is_err() {
                    return;
                }
            }
        });
        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn server_info(&self) -> Result<ServerInfo> {
        self.faults.server_infos.fetch_add(1, Ordering::SeqCst);
        if self.faults.stall_server_info.load(Ordering::SeqCst) {
            std::future::pending::<()>().await;
        }
        if let Some(info) = self.faults.server_info.lock().unwrap().clone() {
            return Ok(info);
        }
        self.inner.server_info().await
    }
}

struct Fixture {
    _server: common::TestServer,
    client: EventStoreClient,
    tenant: String,
}

async fn fixture() -> Fixture {
    let server = spawn_server().await;
    let client = connect(&server.addr).await;
    Fixture {
        client,
        _server: server,
        tenant: unique_tenant(),
    }
}

fn ids_unique(todos: &[String]) -> bool {
    todos.iter().collect::<HashSet<_>>().len() == todos.len()
}

type Task = tokio::task::JoinHandle<(Result<RunExit>, ProjectionRunner<LedgerProjection, Store>)>;

fn spawn_supervised(
    mut runner: ProjectionRunner<LedgerProjection, Store>,
    cancel: &CancellationToken,
    policy: BackoffPolicy,
) -> Task {
    let cancel = cancel.clone();
    tokio::spawn(async move {
        let result = runner.run_supervised(cancel, policy).await;
        (result, runner)
    })
}

/// `run_supervised` that must return within 10 s, so a regression that
/// retries forever fails the test instead of hanging it.
#[async_trait]
trait Bounded {
    async fn run_supervised_bounded(&mut self, policy: BackoffPolicy) -> Result<RunExit>;
}

#[async_trait]
impl Bounded for ProjectionRunner<LedgerProjection, Store> {
    async fn run_supervised_bounded(&mut self, policy: BackoffPolicy) -> Result<RunExit> {
        tokio::time::timeout(
            Duration::from_secs(10),
            self.run_supervised(CancellationToken::new(), policy),
        )
        .await
        .expect("run_supervised must stop on its own")
    }
}

async fn finish(task: Task) -> (Result<RunExit>, ProjectionRunner<LedgerProjection, Store>) {
    tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .expect("runner must stop")
        .expect("runner task panicked")
}

// ---------------------------------------------------------------------------
// Reconnect on UNAVAILABLE
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unavailable_mid_catch_up_resumes_from_checkpoint_without_loss_or_duplicates() {
    let f = fixture().await;
    let mut all = vec![];
    for n in 1..=7 {
        all.push(append(&f.client, &f.tenant, "a", n).await);
    }
    let (port, faults) = FaultPort::wrap(&f.client);
    // Pages of 2: the second page fails after the first committed.
    faults.unavailable_page.store(2, Ordering::SeqCst);
    let store = Arc::new(Store::new());
    let projection = LedgerProjection::default();
    let handled = projection.handled.clone();
    let runner =
        ProjectionRunner::new(port, store.clone(), projection, &f.tenant).with_page_size(2);
    let key = runner.key().clone();
    let mut health = runner.health();
    let cancel = CancellationToken::new();
    let task = spawn_supervised(runner, &cancel, fast());

    wait_health(&mut health, |h| {
        h.state == RunnerState::Live && h.position == all[6]
    })
    .await;
    let h = health.borrow().clone();
    assert_eq!(h.restarts, 1, "{h:?}");
    assert_eq!(h.consecutive_failures, 0, "recovered: {h:?}");
    assert!(h.is_healthy(), "{h:?}");
    assert!(
        h.last_error
            .as_deref()
            .unwrap()
            .contains("replay query failed"),
        "{h:?}"
    );
    cancel.cancel();
    let (result, _) = finish(task).await;
    assert!(
        matches!(result, Ok(RunExit::Cancelled { .. })),
        "{result:?}"
    );

    let state = store.state(&key);
    assert_eq!(
        state.applied, all,
        "every event applied exactly once, in order"
    );
    assert_eq!(handled.load(Ordering::SeqCst), 7, "no event handled twice");
    assert_eq!(faults.subscribes.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn unavailable_mid_live_resumes_without_loss_or_duplicate_effects() {
    let f = fixture().await;
    let g1 = append(&f.client, &f.tenant, "a", 1).await;
    let (port, faults) = FaultPort::wrap(&f.client);
    let store = Arc::new(Store::new());
    let runner = ProjectionRunner::new(port, store.clone(), LedgerProjection::default(), &f.tenant);
    let key = runner.key().clone();
    let mailer = Mailer::new(&store, &key, 0);
    let runner = runner
        .with_live_processor(mailer.clone())
        .with_processor_retry_delay(Duration::from_millis(5));
    let mut health = runner.health();
    // The first subscription dies after one live event.
    faults.live_unavailable_after.store(1, Ordering::SeqCst);
    let cancel = CancellationToken::new();
    let task = spawn_supervised(runner, &cancel, fast());
    wait_health(&mut health, |h| {
        h.state == RunnerState::Live && h.position == g1
    })
    .await;

    let mut all = vec![g1];
    for n in 2..=6 {
        all.push(append(&f.client, &f.tenant, "a", n).await);
    }
    wait_health(&mut health, |h| {
        h.state == RunnerState::Live && h.position == all[5]
    })
    .await;
    let state = store.state(&key);
    eventually("all to-dos executed", || {
        mailer.done().len() == state.todos.len()
    })
    .await;
    cancel.cancel();
    let (result, _) = finish(task).await;
    result.unwrap();

    assert_eq!(state.applied, all);
    assert!(ids_unique(&state.todos), "no duplicate to-do: {state:?}");
    assert_eq!(mailer.effects.load(Ordering::SeqCst), all.len());
    assert_eq!(faults.subscribes.load(Ordering::SeqCst), 2, "one reconnect");
    assert_eq!(health.borrow().restarts, 1);
}

#[tokio::test]
async fn connection_cut_mid_live_reconnects_through_tcp_proxy() {
    let f = fixture().await;
    let proxy = FaultProxy::start(&f._server.addr).await;
    let via_proxy = connect(&proxy.addr).await;
    let mut all = vec![];
    for n in 1..=3 {
        all.push(append(&f.client, &f.tenant, "a", n).await);
    }
    let store = Arc::new(Store::new());
    let runner = ProjectionRunner::new(
        Arc::new(via_proxy),
        store.clone(),
        LedgerProjection::default(),
        &f.tenant,
    );
    let key = runner.key().clone();
    let mailer = Mailer::new(&store, &key, 0);
    let runner = runner
        .with_live_processor(mailer.clone())
        .with_processor_retry_delay(Duration::from_millis(5));
    let mut health = runner.health();
    let cancel = CancellationToken::new();
    let task = spawn_supervised(runner, &cancel, fast());
    wait_health(&mut health, |h| {
        h.state == RunnerState::Live && h.position == all[2]
    })
    .await;
    for n in 4..=5 {
        all.push(append(&f.client, &f.tenant, "a", n).await);
    }
    wait_health(&mut health, |h| h.position == all[4]).await;

    // Outage: events keep committing while the runner cannot reach the store.
    proxy.cut();
    for n in 6..=8 {
        all.push(append(&f.client, &f.tenant, "a", n).await);
    }
    wait_health(&mut health, |h| {
        matches!(h.state, RunnerState::Backoff { .. }) && !h.is_healthy()
    })
    .await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        health.borrow().position,
        all[4],
        "nothing applied during the outage"
    );
    proxy.restore();

    wait_health(&mut health, |h| {
        h.state == RunnerState::Live && h.position == all[7] && h.is_healthy()
    })
    .await;
    let state = store.state(&key);
    eventually("all to-dos executed", || mailer.done().len() == all.len()).await;
    cancel.cancel();
    let (result, _) = finish(task).await;
    result.unwrap();

    assert_eq!(
        state.applied, all,
        "nothing lost or applied twice across the outage"
    );
    assert!(ids_unique(&state.todos));
    assert_eq!(mailer.effects.load(Ordering::SeqCst), all.len());
    assert!(health.borrow().restarts >= 1);
}

// ---------------------------------------------------------------------------
// DATA_LOSS: halt, never skip, never retry with backoff (ADR-026)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn data_loss_live_stops_without_advancing_and_resumes_after_operator_moves_checkpoint() {
    let f = fixture().await;
    let g1 = append(&f.client, &f.tenant, "a", 1).await;
    let g2 = append(&f.client, &f.tenant, "a", 2).await;
    let (port, faults) = FaultPort::wrap(&f.client);
    let store = Arc::new(Store::new());
    let runner = ProjectionRunner::new(
        port.clone(),
        store.clone(),
        LedgerProjection::default(),
        &f.tenant,
    );
    let key = runner.key().clone();
    let mut health = runner.health();
    let cancel = CancellationToken::new();
    let task = spawn_supervised(runner, &cancel, fast());
    wait_health(&mut health, |h| {
        h.state == RunnerState::Live && h.position == g2
    })
    .await;

    faults.bad_after.store(g2, Ordering::SeqCst);
    let g3 = append(&f.client, &f.tenant, "a", 3).await;
    let g4 = append(&f.client, &f.tenant, "a", 4).await;

    let (result, runner) = finish(task).await;
    match result {
        Err(Error::DataLoss { global_nonce, .. }) => assert_eq!(global_nonce, g3),
        other => panic!("expected DataLoss, got {other:?}"),
    }
    assert_eq!(
        store.load_checkpoint(&key).await.unwrap(),
        Some(g2),
        "not advanced past it"
    );
    assert_eq!(store.state(&key).applied, vec![g1, g2]);
    assert_eq!(runner.halted_at(), Some(g3));
    let h = health.borrow().clone();
    assert_eq!(h.state, RunnerState::Halted { global_nonce: g3 });
    assert_eq!(h.halted_at, Some(g3));
    assert!(!h.is_healthy());
    assert_eq!(
        faults.subscribes.load(Ordering::SeqCst),
        1,
        "DATA_LOSS is not retried"
    );
    assert_eq!(
        faults.server_infos.load(Ordering::SeqCst),
        1,
        "one attempt only"
    );

    // Operator: the row is unrecoverable; skip it for this consumer only.
    let tx = store.begin(&key).await.unwrap();
    store.commit(tx, &key, g3).await.unwrap();

    let mut health = runner.health();
    let cancel = CancellationToken::new();
    let task = spawn_supervised(runner, &cancel, fast());
    wait_health(&mut health, |h| {
        h.state == RunnerState::Live && h.position == g4 && h.halted_at.is_none()
    })
    .await;
    cancel.cancel();
    let (result, runner) = finish(task).await;
    result.unwrap();
    assert_eq!(runner.halted_at(), None);
    assert_eq!(
        store.state(&key).applied,
        vec![g1, g2, g4],
        "only the bad event skipped"
    );
}

#[tokio::test]
async fn data_loss_in_catch_up_applies_every_event_before_it() {
    let f = fixture().await;
    let mut all = vec![];
    for n in 1..=6 {
        all.push(append(&f.client, &f.tenant, "a", n).await);
    }
    let (port, faults) = FaultPort::wrap(&f.client);
    // The page of 10 containing it fails as a whole, like Postgres.
    faults.bad.store(all[3], Ordering::SeqCst);
    let store = Arc::new(Store::new());
    let mut runner =
        ProjectionRunner::new(port, store.clone(), LedgerProjection::default(), &f.tenant)
            .with_page_size(10);
    let key = runner.key().clone();
    let err = runner
        .run_supervised_bounded(fast())
        .await
        .expect_err("must halt");
    assert_eq!(err.data_loss_position(), Some(all[3]), "{err:?}");
    assert_eq!(
        store.load_checkpoint(&key).await.unwrap(),
        Some(all[2]),
        "valid events before it are applied, so a skip skips only it"
    );
    assert_eq!(store.state(&key).applied, all[..3].to_vec());
}

#[tokio::test]
async fn undecodable_head_event_is_the_live_boundary() {
    let f = fixture().await;
    let g1 = append(&f.client, &f.tenant, "a", 1).await;
    let g2 = append(&f.client, &f.tenant, "a", 2).await;
    let (port, faults) = FaultPort::wrap(&f.client);
    faults.bad.store(g2, Ordering::SeqCst);
    let store = Arc::new(Store::new());
    let mut runner =
        ProjectionRunner::new(port, store.clone(), LedgerProjection::default(), &f.tenant);
    let key = runner.key().clone();
    let err = runner
        .run_supervised_bounded(fast())
        .await
        .expect_err("must halt");
    assert_eq!(err.data_loss_position(), Some(g2));
    assert_eq!(store.load_checkpoint(&key).await.unwrap(), Some(g1));

    // Operator skip at the head: the head probe still fails on it, and its
    // position serves as the boundary so the runner can go live past it.
    let tx = store.begin(&key).await.unwrap();
    store.commit(tx, &key, g2).await.unwrap();
    let mut health = runner.health();
    let cancel = CancellationToken::new();
    let task = spawn_supervised(runner, &cancel, fast());
    wait_health(&mut health, |h| {
        h.state == RunnerState::Live && h.halted_at.is_none()
    })
    .await;
    let g3 = append(&f.client, &f.tenant, "a", 3).await;
    wait_health(&mut health, |h| h.position == g3).await;
    cancel.cancel();
    finish(task).await.0.unwrap();
    assert_eq!(store.state(&key).applied, vec![g1, g3]);
}

#[tokio::test]
async fn recheck_mode_stays_halted_holds_side_effects_and_resumes_on_its_own() {
    let f = fixture().await;
    let g1 = append(&f.client, &f.tenant, "a", 1).await;
    let (port, faults) = FaultPort::wrap(&f.client);
    let store = Arc::new(Store::new());
    let runner = ProjectionRunner::new(port, store.clone(), LedgerProjection::default(), &f.tenant)
        .with_undecodable_recheck(Duration::from_millis(20));
    let key = runner.key().clone();
    let mailer = Mailer::new(&store, &key, 0);
    let runner = runner
        .with_live_processor(mailer.clone())
        .with_processor_retry_delay(Duration::from_millis(5));
    let mut health = runner.health();
    let cancel = CancellationToken::new();
    let task = spawn_supervised(runner, &cancel, fast());
    wait_health(&mut health, |h| {
        h.state == RunnerState::Live && h.position == g1
    })
    .await;
    eventually("startup pass ran", || mailer.done().len() == 1).await;

    faults.bad_after.store(g1, Ordering::SeqCst);
    let g2 = append(&f.client, &f.tenant, "a", 2).await;
    let g3 = append(&f.client, &f.tenant, "a", 3).await;
    wait_health(&mut health, |h| h.halted_at == Some(g2)).await;

    // Halted: re-checks happen, nothing advances, no side effects run.
    let passes = mailer.passes.load(Ordering::SeqCst);
    let reads = faults.read_alls.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(!task.is_finished(), "recheck mode keeps running");
    assert!(
        faults.read_alls.load(Ordering::SeqCst) >= reads + 3,
        "re-checks at the fixed interval"
    );
    assert_eq!(
        mailer.passes.load(Ordering::SeqCst),
        passes,
        "processor held while halted"
    );
    assert_eq!(store.load_checkpoint(&key).await.unwrap(), Some(g1));
    let h = health.borrow().clone();
    assert_eq!(h.halted_at, Some(g2));
    assert!(!h.is_healthy());

    // Repaired (e.g. an event store that decodes the row): resumes on its own.
    faults.bad_after.store(0, Ordering::SeqCst);
    faults.bad.store(0, Ordering::SeqCst);
    wait_health(&mut health, |h| {
        h.state == RunnerState::Live && h.position == g3 && h.halted_at.is_none() && h.is_healthy()
    })
    .await;
    eventually("pending to-dos executed after the halt", || {
        mailer.done().len() == 3
    })
    .await;
    cancel.cancel();
    finish(task).await.0.unwrap();
    assert_eq!(store.state(&key).applied, vec![g1, g2, g3]);
    assert_eq!(mailer.effects.load(Ordering::SeqCst), 3);
}

// ---------------------------------------------------------------------------
// Live processor panics
// ---------------------------------------------------------------------------

#[tokio::test]
async fn panicking_processor_stops_the_runner_with_a_typed_error() {
    let f = fixture().await;
    append(&f.client, &f.tenant, "a", 1).await;
    let (port, faults) = FaultPort::wrap(&f.client);
    let store = Arc::new(Store::new());
    let runner = ProjectionRunner::new(port, store.clone(), LedgerProjection::default(), &f.tenant);
    let mailer = Mailer::new(&store, runner.key(), 1);
    let runner = runner.with_live_processor(mailer.clone());
    let mut health = runner.health();
    let (result, _) = finish(spawn_supervised(runner, &CancellationToken::new(), fast())).await;
    match result {
        Err(Error::LiveProcessorPanicked { message, .. }) => {
            assert!(message.contains("mailer exploded"), "{message}");
        }
        other => panic!("expected LiveProcessorPanicked, got {other:?}"),
    }
    assert_eq!(
        faults.server_infos.load(Ordering::SeqCst),
        1,
        "a panic is not retried"
    );
    let h = health.borrow_and_update().clone();
    assert_eq!(h.state, RunnerState::Failed);
    assert!(h.last_error.unwrap().contains("panicked"));
}

#[tokio::test]
async fn restart_policy_retries_a_panicked_pass() {
    let f = fixture().await;
    let g1 = append(&f.client, &f.tenant, "a", 1).await;
    let store = Arc::new(Store::new());
    let runner = ProjectionRunner::new(
        Arc::new(f.client.clone()),
        store.clone(),
        LedgerProjection::default(),
        &f.tenant,
    );
    let mailer = Mailer::new(&store, runner.key(), 2);
    let runner = runner
        .with_live_processor(mailer.clone())
        .with_processor_retry_delay(Duration::from_millis(5))
        .on_processor_panic(ProcessorPanicPolicy::Restart);
    let mut health = runner.health();
    let cancel = CancellationToken::new();
    let task = spawn_supervised(runner, &cancel, fast());
    wait_health(&mut health, |h| {
        h.state == RunnerState::Live && h.position == g1
    })
    .await;
    eventually("processed after two panics", || mailer.done().len() == 1).await;
    assert!(mailer.passes.load(Ordering::SeqCst) >= 3);
    assert!(!task.is_finished());
    cancel.cancel();
    finish(task).await.0.unwrap();
}

/// Hand-written `LiveProcessor` that panics while creating the pass future
/// (before any `.await`), outside the polled future.
struct PanicsBeforeFuture;

impl LiveProcessor for PanicsBeforeFuture {
    fn process_pending<'a, 'b>(
        &'a self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<usize>> + Send + 'b>>
    where
        'a: 'b,
        Self: 'b,
    {
        panic!("broken before the first poll");
    }
}

#[tokio::test]
async fn processor_panicking_before_its_future_exists_still_stops_the_runner() {
    let f = fixture().await;
    append(&f.client, &f.tenant, "a", 1).await;
    let runner = ProjectionRunner::new(
        Arc::new(f.client.clone()),
        Arc::new(Store::new()),
        LedgerProjection::default(),
        &f.tenant,
    )
    .with_live_processor(Arc::new(PanicsBeforeFuture));
    let (result, _) = finish(spawn_supervised(runner, &CancellationToken::new(), fast())).await;
    match result {
        Err(Error::LiveProcessorPanicked { message, .. }) => {
            assert!(
                message.contains("broken before the first poll"),
                "{message}"
            );
        }
        other => panic!("expected LiveProcessorPanicked, got {other:?}"),
    }
}

/// Blocks inside `process_pending` until released; counts completions.
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
async fn shutdown_cancels_a_stalled_pass_after_the_grace_period() {
    let f = fixture().await;
    append(&f.client, &f.tenant, "a", 1).await;
    let processor = Arc::new(BlockingProcessor::default());
    let runner = ProjectionRunner::new(
        Arc::new(f.client.clone()),
        Arc::new(Store::new()),
        LedgerProjection::default(),
        &f.tenant,
    )
    .with_live_processor(processor.clone())
    .with_processor_shutdown_grace(Duration::from_millis(100));
    let cancel = CancellationToken::new();
    let task = spawn_supervised(runner, &cancel, fast());
    eventually("pass started", || {
        processor.started.load(Ordering::SeqCst) == 1
    })
    .await;
    cancel.cancel();
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert!(!task.is_finished(), "the pass gets its grace period");
    let (result, _) = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .expect("a stalled pass must not block shutdown past the grace period")
        .unwrap();
    assert!(
        matches!(result, Ok(RunExit::Cancelled { .. })),
        "{result:?}"
    );
    processor.gate.notify_waiters();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        processor.completed.load(Ordering::SeqCst),
        0,
        "pass cancelled"
    );
}

#[tokio::test]
async fn data_loss_halts_promptly_and_cancels_an_in_flight_pass() {
    let f = fixture().await;
    let g1 = append(&f.client, &f.tenant, "a", 1).await;
    let (port, faults) = FaultPort::wrap(&f.client);
    let processor = Arc::new(BlockingProcessor::default());
    let runner = ProjectionRunner::new(
        port,
        Arc::new(Store::new()),
        LedgerProjection::default(),
        &f.tenant,
    )
    .with_live_processor(processor.clone());
    let mut health = runner.health();
    let task = spawn_supervised(runner, &CancellationToken::new(), fast());
    wait_health(&mut health, |h| {
        h.state == RunnerState::Live && h.position == g1
    })
    .await;
    // The startup pass is stuck on an "external service".
    eventually("pass started", || {
        processor.started.load(Ordering::SeqCst) == 1
    })
    .await;

    faults.bad_after.store(g1, Ordering::SeqCst);
    let g2 = append(&f.client, &f.tenant, "a", 2).await;
    let (result, _) = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .expect("a blocked processor pass must not delay the halt")
        .unwrap();
    assert_eq!(result.unwrap_err().data_loss_position(), Some(g2));
    assert_eq!(health.borrow().halted_at, Some(g2));

    // The cancelled pass never completes its side effect.
    processor.gate.notify_waiters();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(processor.completed.load(Ordering::SeqCst), 0);
}

// ---------------------------------------------------------------------------
// Capability guard
// ---------------------------------------------------------------------------

/// Port that does not implement `server_info`: the trait default reports a
/// legacy server.
struct NoServerInfo(EventStoreClient);

#[async_trait]
impl EventStorePort for NoServerInfo {
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
    async fn subscribe(&self, req: proto::SubscribeRequest) -> Result<EventDataStream> {
        self.0.subscribe(req).await
    }
}

#[tokio::test]
async fn capability_guard_refuses_legacy_and_partial_servers() {
    let f = fixture().await;
    let g1 = append(&f.client, &f.tenant, "a", 1).await;

    // Legacy server (no GetServerInfo), via the trait default.
    let store = Arc::new(Store::new());
    let mut runner = ProjectionRunner::new(
        Arc::new(NoServerInfo(f.client.clone())),
        store.clone(),
        LedgerProjection::default(),
        &f.tenant,
    );
    let err = runner
        .run_supervised_bounded(fast())
        .await
        .expect_err("legacy server refused");
    assert!(matches!(err, Error::Incompatible(_)), "{err:?}");
    assert!(
        err.to_string().contains("commit_ordered_global_nonce"),
        "{err}"
    );
    assert_eq!(store.load_checkpoint(runner.key()).await.unwrap(), None);
    assert!(runner.catch_up().await.is_err(), "catch_up is guarded too");

    // A server missing one capability; nothing is read.
    let (port, faults) = FaultPort::wrap(&f.client);
    *faults.server_info.lock().unwrap() = Some(ServerInfo {
        server_version: Some("0.17.0".into()),
        api_version: Some("eventstore.v1".into()),
        backend: Some("custom".into()),
        capabilities: vec![
            capabilities::COMMIT_ORDERED_GLOBAL_NONCE.into(),
            capabilities::SUBSCRIPTION_ERRORS_SURFACED.into(),
        ],
    });
    let mut runner = ProjectionRunner::new(
        port.clone(),
        Arc::new(Store::new()),
        LedgerProjection::default(),
        &f.tenant,
    );
    let err = runner
        .run_supervised_bounded(fast())
        .await
        .expect_err("partial server refused");
    match &err {
        Error::Incompatible(
            event_sourcing_rust::client::CompatibilityError::MissingCapabilities {
                missing, ..
            },
        ) => assert_eq!(
            missing,
            &vec![capabilities::UNDECODABLE_EVENTS_SURFACED.to_string()]
        ),
        other => panic!("expected MissingCapabilities, got {other:?}"),
    }
    assert_eq!(faults.read_alls.load(Ordering::SeqCst), 0, "nothing read");
    assert_eq!(faults.server_infos.load(Ordering::SeqCst), 1, "not retried");

    // A narrower requirement, or opting out, runs.
    let mut narrower = ProjectionRunner::new(
        port.clone(),
        Arc::new(Store::new()),
        LedgerProjection::default(),
        &f.tenant,
    )
    .with_required_capabilities([capabilities::COMMIT_ORDERED_GLOBAL_NONCE]);
    assert_eq!(narrower.catch_up().await.unwrap(), g1);
    let mut opted_out = ProjectionRunner::new(
        Arc::new(NoServerInfo(f.client.clone())),
        Arc::new(Store::new()),
        LedgerProjection::default(),
        &f.tenant,
    )
    .without_capability_check();
    assert_eq!(opted_out.catch_up().await.unwrap(), g1);

    // The real server advertises all three.
    let mut real = ProjectionRunner::new(
        Arc::new(f.client.clone()),
        Arc::new(Store::new()),
        LedgerProjection::default(),
        &f.tenant,
    );
    assert_eq!(real.catch_up().await.unwrap(), g1);
}

// ---------------------------------------------------------------------------
// Backoff, cancellation, retry classification
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cancellation_during_a_stalled_startup_call_is_prompt() {
    let f = fixture().await;
    let (port, faults) = FaultPort::wrap(&f.client);
    faults.stall_server_info.store(true, Ordering::SeqCst);
    let runner = ProjectionRunner::new(
        port,
        Arc::new(Store::new()),
        LedgerProjection::default(),
        &f.tenant,
    );
    let cancel = CancellationToken::new();
    let task = spawn_supervised(runner, &cancel, fast());
    eventually("capability probe sent", || {
        faults.server_infos.load(Ordering::SeqCst) == 1
    })
    .await;
    cancel.cancel();
    let (result, _) = tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .expect("cancellation during a stalled call must be prompt")
        .unwrap();
    assert!(
        matches!(result, Ok(RunExit::Cancelled { position: 0 })),
        "{result:?}"
    );
}

#[tokio::test]
async fn backslash_feed_prefix_requires_literal_prefix_capability() {
    let f = fixture().await;
    let (port, faults) = FaultPort::wrap(&f.client);
    // A server with the default three, but matching prefixes with LIKE.
    *faults.server_info.lock().unwrap() = Some(ServerInfo {
        server_version: Some("0.17.0".into()),
        api_version: Some("eventstore.v1".into()),
        backend: Some("postgres".into()),
        capabilities: vec![
            capabilities::COMMIT_ORDERED_GLOBAL_NONCE.into(),
            capabilities::SUBSCRIPTION_ERRORS_SURFACED.into(),
            capabilities::UNDECODABLE_EVENTS_SURFACED.into(),
        ],
    });
    let new_runner = |feed: &str| {
        ProjectionRunner::new(
            port.clone(),
            Arc::new(Store::new()),
            LedgerProjection::default(),
            &f.tenant,
        )
        .with_feed_prefix(feed)
    };
    let err = new_runner("acct\\")
        .run_supervised_bounded(fast())
        .await
        .expect_err("refused");
    assert!(
        err.to_string()
            .contains(capabilities::LITERAL_SUBSCRIPTION_PREFIX),
        "{err}"
    );
    // `%` and `_` only widen a LIKE match; the runner filters exactly.
    assert_eq!(new_runner("acct_").catch_up().await.unwrap(), 0);
}

#[tokio::test]
async fn cancellation_during_backoff_is_prompt() {
    let f = fixture().await;
    let (port, faults) = FaultPort::wrap(&f.client);
    faults.always_unavailable.store(true, Ordering::SeqCst);
    let runner = ProjectionRunner::new(
        port,
        Arc::new(Store::new()),
        LedgerProjection::default(),
        &f.tenant,
    );
    let mut health = runner.health();
    let cancel = CancellationToken::new();
    let policy =
        BackoffPolicy::new(Duration::from_secs(60), Duration::from_secs(60)).with_jitter(0.0);
    let task = spawn_supervised(runner, &cancel, policy);
    wait_health(&mut health, |h| {
        matches!(
            h.state,
            RunnerState::Backoff { attempt: 1, delay } if delay == Duration::from_secs(60)
        )
    })
    .await;
    let started = tokio::time::Instant::now();
    cancel.cancel();
    let (result, _) = tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .expect("cancellation during backoff must be prompt")
        .unwrap();
    assert!(
        matches!(result, Ok(RunExit::Cancelled { position: 0 })),
        "{result:?}"
    );
    assert!(started.elapsed() < Duration::from_millis(500));
    assert_eq!(health.borrow().state, RunnerState::Stopped);
}

#[tokio::test]
async fn backoff_grows_between_attempts_and_max_retries_gives_up() {
    let f = fixture().await;
    let (port, faults) = FaultPort::wrap(&f.client);
    faults.always_unavailable.store(true, Ordering::SeqCst);
    let mut runner = ProjectionRunner::new(
        port,
        Arc::new(Store::new()),
        LedgerProjection::default(),
        &f.tenant,
    );
    let mut health = runner.health();
    let delays = Arc::new(Mutex::new(Vec::new()));
    let watcher = tokio::spawn({
        let delays = delays.clone();
        async move {
            while health.changed().await.is_ok() {
                if let RunnerState::Backoff { attempt, delay } = health.borrow_and_update().state {
                    delays.lock().unwrap().push((attempt, delay));
                }
            }
        }
    });
    let policy = BackoffPolicy::new(Duration::from_millis(5), Duration::from_millis(20))
        .with_jitter(0.0)
        .with_max_retries(3);
    let err = runner
        .run_supervised_bounded(policy)
        .await
        .expect_err("gives up");
    assert!(err.is_transient(), "{err:?}");
    assert_eq!(
        faults.server_infos.load(Ordering::SeqCst),
        4,
        "1 attempt + 3 retries"
    );
    drop(runner);
    let _ = watcher.await;
    let delays = delays.lock().unwrap().clone();
    assert_eq!(
        delays,
        vec![
            (1, Duration::from_millis(5)),
            (2, Duration::from_millis(10)),
            (3, Duration::from_millis(20)),
        ]
    );
}

#[tokio::test]
async fn handler_errors_are_not_retried() {
    let f = fixture().await;
    let g1 = append(&f.client, &f.tenant, "a", 1).await;
    let g2 = append(&f.client, &f.tenant, "a", 2).await;
    let (port, faults) = FaultPort::wrap(&f.client);
    let store = Arc::new(Store::new());
    let projection = LedgerProjection {
        fail_at: Some(g2),
        ..LedgerProjection::default()
    };
    let mut runner = ProjectionRunner::new(port, store.clone(), projection, &f.tenant);
    let err = runner
        .run_supervised_bounded(fast())
        .await
        .expect_err("handler failure stops");
    assert!(
        matches!(err, Error::ProjectionFailed { global_nonce, .. } if global_nonce == g2),
        "{err:?}"
    );
    assert_eq!(faults.server_infos.load(Ordering::SeqCst), 1);
    assert_eq!(store.load_checkpoint(runner.key()).await.unwrap(), Some(g1));
    assert_eq!(runner.health().borrow().state, RunnerState::Failed);
}

// ---------------------------------------------------------------------------
// Real undecodable row (Postgres backend only)
// ---------------------------------------------------------------------------

/// Insert a row the Postgres backend cannot decode (headers hold a number
/// where a string is required), as `it_subscribe_undecodable.rs` does.
#[cfg(feature = "postgres")]
async fn insert_undecodable_row(url: &str, tenant: &str) -> u64 {
    let pool = sqlx::PgPool::connect(url).await.expect("connect postgres");
    let global: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO events (
            tenant_id, aggregate_id, aggregate_type, aggregate_nonce,
            event_id, event_type, event_version, content_type,
            recorded_time_unix_ms, headers, payload
        ) VALUES ($1, 'bad', 'Account', 1, $2, 'Deposited', 1,
                  'application/json', 0, '{"k": 1}'::jsonb, $3)
        RETURNING global_nonce
        "#,
    )
    .bind(tenant)
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(br#"{"amount":1}"#.to_vec())
    .fetch_one(&pool)
    .await
    .expect("insert undecodable row");
    global as u64
}

#[cfg(feature = "postgres")]
#[tokio::test]
async fn postgres_undecodable_row_halts_with_position_from_trailing_metadata() {
    let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
        // Opt-in locally; under CI (`make test-postgres`) it must run.
        assert!(
            std::env::var_os("CI").is_none(),
            "TEST_DATABASE_URL must be set when running postgres-feature tests in CI"
        );
        eprintln!("skipping: TEST_DATABASE_URL not set (memory backend has no undecodable rows)");
        return;
    };
    let f = fixture().await;
    let g1 = append(&f.client, &f.tenant, "a", 1).await;
    let g2 = append(&f.client, &f.tenant, "a", 2).await;
    let bad = insert_undecodable_row(&url, &f.tenant).await;
    let g3 = append(&f.client, &f.tenant, "a", 3).await;

    let store = Arc::new(Store::new());
    let mut runner = ProjectionRunner::new(
        Arc::new(f.client.clone()),
        store.clone(),
        LedgerProjection::default(),
        &f.tenant,
    );
    let key = runner.key().clone();
    let err = runner
        .run_supervised_bounded(fast())
        .await
        .expect_err("undecodable row halts");
    assert_eq!(err.data_loss_position(), Some(bad), "{err:?}");
    assert_eq!(store.load_checkpoint(&key).await.unwrap(), Some(g2));
    assert_eq!(runner.halted_at(), Some(bad));

    let tx = store.begin(&key).await.unwrap();
    store.commit(tx, &key, bad).await.unwrap();
    assert_eq!(runner.catch_up().await.unwrap(), g3);
    assert_eq!(runner.halted_at(), None);
    assert_eq!(store.state(&key).applied, vec![g1, g2, g3]);
}
