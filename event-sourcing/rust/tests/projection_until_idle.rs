//! Live-server tests for `ProjectionRunner::run_until_idle`: a to-do list
//! automation run as a batch (Dilger, *Understanding Eventsourcing*,
//! pp.497-508). The projection records pending work, the processor issues
//! one command (an append) per entry, and the resulting `Done` event clears
//! the entry through the same runner.

mod common;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::Notify;

use async_trait::async_trait;
use common::{connect, spawn_server, take, unique_tenant};
use event_sourcing_rust::client::{proto, EventStoreClient, EventStorePort};
use event_sourcing_rust::error::{Error, Result};
use event_sourcing_rust::projection::{
    CheckpointKey, CheckpointedProjection, DispatchContext, IdleExit, InMemoryProjectionStore,
    InMemoryTx, LiveProcessor, ProjectionRunner, RecordedEvent, RunnerState,
};
use tokio_util::sync::CancellationToken;

const REQUESTED: &str = "WorkRequested";
const DONE: &str = "WorkDone";

/// Pending entries: work id -> aggregate nonce of the request.
#[derive(Debug, Clone, Default, PartialEq)]
struct Todo {
    pending: BTreeMap<String, u64>,
}

type TodoStore = InMemoryProjectionStore<Todo>;

struct TodoList;

#[async_trait]
impl CheckpointedProjection<TodoStore> for TodoList {
    fn name(&self) -> &str {
        "work-todo"
    }
    fn version(&self) -> u32 {
        1
    }
    async fn handle(
        &mut self,
        tx: &mut InMemoryTx<Todo>,
        event: &RecordedEvent,
        _ctx: &DispatchContext,
    ) -> Result<()> {
        match event.event_type.as_str() {
            REQUESTED => {
                tx.state
                    .pending
                    .insert(event.aggregate_id.clone(), event.aggregate_nonce);
            }
            DONE => {
                tx.state.pending.remove(&event.aggregate_id);
            }
            _ => {}
        }
        Ok(())
    }
    async fn reset(&mut self, tx: &mut InMemoryTx<Todo>, _key: &CheckpointKey) -> Result<()> {
        tx.state = Todo::default();
        Ok(())
    }
}

async fn append(
    client: &EventStoreClient,
    tenant: &str,
    id: &str,
    nonce: u64,
    ty: &str,
) -> Result<u64> {
    client
        .append(proto::AppendRequest {
            tenant_id: tenant.into(),
            aggregate_id: id.into(),
            aggregate_type: "Work".into(),
            expected_aggregate_nonce: nonce - 1,
            idempotency_key: format!("{ty}:{id}:{nonce}"),
            events: vec![proto::EventData {
                meta: Some(proto::EventMetadata {
                    event_id: uuid::Uuid::new_v4().to_string(),
                    aggregate_id: id.into(),
                    aggregate_type: "Work".into(),
                    aggregate_nonce: nonce,
                    event_type: ty.into(),
                    event_version: 1,
                    content_type: "application/json".into(),
                    tenant_id: tenant.into(),
                    ..Default::default()
                }),
                payload: b"{}".to_vec(),
            }],
        })
        .await
        .map(|r| r.last_global_nonce)
}

#[derive(Default)]
struct Behaviour {
    /// Fail this many passes before doing anything.
    fail_passes: AtomicU32,
    /// Report work without issuing commands (never clears the list).
    runaway: bool,
    /// Panic in the pass.
    panic: bool,
    /// Another writer: during the first pass, append a request for this id
    /// (after the pass read its list), as if concurrently.
    inject_once: Mutex<Option<String>>,
    /// Signal `started`, then hold the pass.
    hold: Option<Hold>,
}

enum Hold {
    /// Never finish (only cancellation ends it).
    Forever(Arc<Notify>),
    /// Wait for `release`, then fail.
    ThenFail {
        started: Arc<Notify>,
        release: Arc<Notify>,
    },
}

/// Completes each pending entry by appending `Done` after the request.
struct Completer {
    client: EventStoreClient,
    tenant: String,
    store: Arc<TodoStore>,
    key: CheckpointKey,
    behaviour: Behaviour,
    passes: AtomicUsize,
    /// Pending entries each pass saw, in order.
    seen: Mutex<Vec<usize>>,
}

impl Completer {
    fn new(f: &Fixture, store: &Arc<TodoStore>, behaviour: Behaviour) -> Arc<Self> {
        Arc::new(Self {
            client: f.client.clone(),
            tenant: f.tenant.clone(),
            store: store.clone(),
            key: CheckpointKey::new(&f.tenant, "work-todo", 1),
            behaviour,
            passes: AtomicUsize::new(0),
            seen: Mutex::new(Vec::new()),
        })
    }

    /// For a runner whose feed is narrowed: the list lives under that key.
    fn for_feed(f: &Fixture, store: &Arc<TodoStore>, feed: &str) -> Arc<Self> {
        let mut completer = Self::new(f, store, Behaviour::default());
        Arc::get_mut(&mut completer).expect("unshared").key =
            CheckpointKey::new(&f.tenant, "work-todo", 1).with_feed(feed);
        completer
    }
}

#[async_trait]
impl LiveProcessor for Completer {
    async fn process_pending(&self) -> Result<usize> {
        self.passes.fetch_add(1, Ordering::SeqCst);
        if self.behaviour.panic {
            panic!("processor bug");
        }
        if take(&self.behaviour.fail_passes) {
            return Err(Error::from(tonic::Status::unavailable("downstream down")));
        }
        match &self.behaviour.hold {
            Some(Hold::Forever(started)) => {
                started.notify_one();
                std::future::pending::<()>().await;
            }
            Some(Hold::ThenFail { started, release }) => {
                started.notify_one();
                release.notified().await;
                return Err(Error::from(tonic::Status::internal("failed during grace")));
            }
            None => {}
        }
        let pending = self.store.state(&self.key).pending;
        self.seen.lock().unwrap().push(pending.len());
        let injected = self.behaviour.inject_once.lock().unwrap().take();
        if let Some(id) = injected {
            append(&self.client, &self.tenant, &id, 1, REQUESTED).await?;
        }
        if self.behaviour.runaway {
            return Ok(pending.len().max(1));
        }
        let mut done = 0;
        for (id, nonce) in pending {
            match append(&self.client, &self.tenant, &id, nonce + 1, DONE).await {
                Ok(_) => done += 1,
                // Already done by an earlier pass whose event is not projected yet.
                Err(Error::ConcurrencyConflict { .. }) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(done)
    }
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

async fn request(f: &Fixture, ids: &[&str]) {
    for id in ids {
        append(&f.client, &f.tenant, id, 1, REQUESTED)
            .await
            .expect("request");
    }
}

async fn head(f: &Fixture) -> u64 {
    f.client
        .read_all(proto::ReadAllRequest {
            tenant_id: f.tenant.clone(),
            from_global_nonce: u64::MAX >> 1,
            max_count: 1,
            forward: false,
        })
        .await
        .expect("head")
        .events
        .first()
        .and_then(|e| e.meta.as_ref())
        .map(|m| m.global_nonce)
        .unwrap_or(0)
}

async fn done_count(f: &Fixture) -> usize {
    f.client
        .read_all(proto::ReadAllRequest {
            tenant_id: f.tenant.clone(),
            from_global_nonce: 1,
            max_count: 1000,
            forward: true,
        })
        .await
        .expect("read")
        .events
        .iter()
        .filter(|e| e.meta.as_ref().is_some_and(|m| m.event_type == DONE))
        .count()
}

fn runner(
    f: &Fixture,
    store: &Arc<TodoStore>,
    processor: Option<Arc<dyn LiveProcessor>>,
) -> ProjectionRunner<TodoList, TodoStore> {
    let r = ProjectionRunner::new(f.port.clone(), store.clone(), TodoList, &f.tenant);
    match processor {
        Some(p) => r.with_live_processor(p),
        None => r,
    }
}

fn pending(store: &TodoStore, f: &Fixture) -> usize {
    store
        .state(&CheckpointKey::new(&f.tenant, "work-todo", 1))
        .pending
        .len()
}

#[tokio::test]
async fn drains_the_list_and_returns_idle_with_the_back_channel_folded_in() {
    let f = fixture().await;
    request(&f, &["a", "b", "c"]).await;
    let store = Arc::new(TodoStore::new());
    let completer = Completer::new(&f, &store, Behaviour::default());
    let mut runner = runner(&f, &store, Some(completer.clone()));

    let exit = runner
        .run_until_idle(CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(
        exit,
        IdleExit::Idle {
            position: 6,
            processed: 3,
            passes: 2
        }
    );
    assert_eq!(pending(&store, &f), 0, "each Done cleared its entry");
    assert_eq!(done_count(&f).await, 3);
    assert_eq!(
        *completer.seen.lock().unwrap(),
        vec![3, 0],
        "the first pass saw the fully caught-up list, the second an empty one"
    );
}

#[tokio::test]
async fn a_second_call_over_an_unchanged_log_issues_nothing() {
    let f = fixture().await;
    request(&f, &["a", "b"]).await;
    let store = Arc::new(TodoStore::new());
    let completer = Completer::new(&f, &store, Behaviour::default());
    let mut runner = runner(&f, &store, Some(completer));
    runner
        .run_until_idle(CancellationToken::new())
        .await
        .unwrap();
    let before = head(&f).await;

    let again = runner
        .run_until_idle(CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(
        again,
        IdleExit::Idle {
            position: before,
            processed: 0,
            passes: 1
        }
    );
    assert_eq!(head(&f).await, before, "nothing appended");
}

#[tokio::test]
async fn without_a_processor_it_is_a_catch_up() {
    let f = fixture().await;
    request(&f, &["a", "b"]).await;
    let store = Arc::new(TodoStore::new());
    let mut runner = runner(&f, &store, None);

    let exit = runner
        .run_until_idle(CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(
        exit,
        IdleExit::Idle {
            position: 2,
            processed: 0,
            passes: 0
        }
    );
    assert_eq!(pending(&store, &f), 2);
}

#[tokio::test]
async fn a_failed_pass_returns_its_error_and_the_next_call_finishes_once() {
    let f = fixture().await;
    request(&f, &["a", "b"]).await;
    let store = Arc::new(TodoStore::new());
    let failing = Completer::new(
        &f,
        &store,
        Behaviour {
            fail_passes: AtomicU32::new(1),
            ..Default::default()
        },
    );
    let mut runner = runner(&f, &store, Some(failing));

    let err = runner
        .run_until_idle(CancellationToken::new())
        .await
        .unwrap_err();
    assert!(err.is_transient(), "{err}");
    assert_eq!(done_count(&f).await, 0, "nothing done before the failure");
    assert_eq!(
        runner.position(),
        2,
        "the catch-up before the pass committed"
    );

    let exit = runner
        .run_until_idle(CancellationToken::new())
        .await
        .unwrap();
    assert!(
        matches!(exit, IdleExit::Idle { processed: 2, .. }),
        "{exit:?}"
    );
    assert_eq!(done_count(&f).await, 2, "each entry completed exactly once");
}

#[tokio::test]
async fn a_processor_that_never_clears_its_list_hits_the_pass_bound() {
    let f = fixture().await;
    request(&f, &["a"]).await;
    let store = Arc::new(TodoStore::new());
    let runaway = Completer::new(
        &f,
        &store,
        Behaviour {
            runaway: true,
            ..Default::default()
        },
    );
    let mut runner = runner(&f, &store, Some(runaway.clone())).with_idle_max_passes(3);

    let exit = runner
        .run_until_idle(CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(
        exit,
        IdleExit::PassLimit {
            position: 1,
            processed: 3
        }
    );
    assert_eq!(runaway.passes.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn a_panicking_pass_is_reported_not_unwound() {
    let f = fixture().await;
    request(&f, &["a"]).await;
    let store = Arc::new(TodoStore::new());
    let panicking = Completer::new(
        &f,
        &store,
        Behaviour {
            panic: true,
            ..Default::default()
        },
    );
    let mut runner = runner(&f, &store, Some(panicking));

    let err = runner
        .run_until_idle(CancellationToken::new())
        .await
        .unwrap_err();

    match err {
        Error::LiveProcessorPanicked { message, .. } => {
            assert!(message.contains("processor bug"), "{message}")
        }
        other => panic!("expected LiveProcessorPanicked, got {other}"),
    }
}

#[tokio::test]
async fn a_cancelled_call_returns_cancelled_and_commits_nothing() {
    let f = fixture().await;
    request(&f, &["a"]).await;
    let store = Arc::new(TodoStore::new());
    let completer = Completer::new(&f, &store, Behaviour::default());
    let mut runner = runner(&f, &store, Some(completer.clone()));
    let cancel = CancellationToken::new();
    cancel.cancel();

    let exit = tokio::time::timeout(Duration::from_secs(5), runner.run_until_idle(cancel))
        .await
        .expect("prompt")
        .unwrap();

    assert_eq!(exit, IdleExit::Cancelled { position: 0 });
    assert_eq!(completer.passes.load(Ordering::SeqCst), 0);
    assert_eq!(done_count(&f).await, 0);
}

#[tokio::test]
async fn an_event_appended_during_an_empty_pass_is_not_mistaken_for_idle() {
    let f = fixture().await;
    let store = Arc::new(TodoStore::new());
    let completer = Completer::new(
        &f,
        &store,
        Behaviour {
            inject_once: Mutex::new(Some("late".into())),
            ..Default::default()
        },
    );
    let mut runner = runner(&f, &store, Some(completer.clone()));

    let exit = runner
        .run_until_idle(CancellationToken::new())
        .await
        .unwrap();

    // Pass 1 saw an empty list and took nothing, but a request arrived
    // meanwhile: the next catch-up sees it, pass 2 takes it, pass 3 is empty.
    assert_eq!(
        exit,
        IdleExit::Idle {
            position: 2,
            processed: 1,
            passes: 3
        }
    );
    assert_eq!(*completer.seen.lock().unwrap(), vec![0, 1, 0]);
    assert_eq!(done_count(&f).await, 1);
    assert_eq!(
        runner.health().borrow().state,
        RunnerState::Stopped,
        "idle is a clean stop"
    );
}

#[tokio::test]
async fn a_feed_prefix_below_the_tenant_head_still_reaches_idle() {
    let f = fixture().await;
    request(&f, &["w-a"]).await;
    // Another family after it: outside the feed, so the checkpoint stays
    // below the tenant head.
    append(&f.client, &f.tenant, "x-1", 1, "Other")
        .await
        .unwrap();
    let store = Arc::new(TodoStore::new());
    let completer = Completer::for_feed(&f, &store, "w-");
    let mut runner = ProjectionRunner::new(f.port.clone(), store.clone(), TodoList, &f.tenant)
        .with_feed_prefix("w-")
        .with_live_processor(completer)
        .with_idle_max_passes(5);

    let exit = runner
        .run_until_idle(CancellationToken::new())
        .await
        .unwrap();

    assert!(
        matches!(
            exit,
            IdleExit::Idle {
                processed: 1,
                passes: 2,
                ..
            }
        ),
        "{exit:?}"
    );
    assert_eq!(done_count(&f).await, 1);
}

#[tokio::test]
async fn cancelling_a_held_pass_returns_after_the_grace() {
    let f = fixture().await;
    request(&f, &["a"]).await;
    let store = Arc::new(TodoStore::new());
    let started = Arc::new(Notify::new());
    let held = Completer::new(
        &f,
        &store,
        Behaviour {
            hold: Some(Hold::Forever(started.clone())),
            ..Default::default()
        },
    );
    let mut runner =
        runner(&f, &store, Some(held)).with_processor_shutdown_grace(Duration::from_millis(50));
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        started.notified().await;
        trigger.cancel();
    });

    let exit = tokio::time::timeout(Duration::from_secs(5), runner.run_until_idle(cancel))
        .await
        .expect("returns after the grace")
        .unwrap();

    assert_eq!(exit, IdleExit::Cancelled { position: 1 });
    assert_eq!(done_count(&f).await, 0);
}

#[tokio::test]
async fn a_pass_that_fails_within_the_grace_reports_its_error() {
    let f = fixture().await;
    request(&f, &["a"]).await;
    let store = Arc::new(TodoStore::new());
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let held = Completer::new(
        &f,
        &store,
        Behaviour {
            hold: Some(Hold::ThenFail {
                started: started.clone(),
                release: release.clone(),
            }),
            ..Default::default()
        },
    );
    let mut runner =
        runner(&f, &store, Some(held)).with_processor_shutdown_grace(Duration::from_secs(5));
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        started.notified().await;
        trigger.cancel();
        tokio::time::sleep(Duration::from_millis(20)).await;
        release.notify_one();
    });

    let err = tokio::time::timeout(Duration::from_secs(5), runner.run_until_idle(cancel))
        .await
        .expect("returns")
        .unwrap_err();

    assert!(err.to_string().contains("failed during grace"), "{err}");
}
