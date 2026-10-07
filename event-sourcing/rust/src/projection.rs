//! Checkpointed projections (ADR-014) and the live-only processor side of the
//! process-manager pattern (ADR-025).
//!
//! # Model
//!
//! A [`ProjectionRunner`] feeds one [`CheckpointedProjection`] from one
//! tenant's global log and persists its resume position through a
//! [`ProjectionStore`].
//!
//! * **Checkpoint identity** is a [`CheckpointKey`]: tenant, projection name,
//!   projection version, and feed filter. Two projections, two tenants, or
//!   two versions of one projection never share a position.
//! * **Catch-up** reads the log with `read_all` up to the head observed at
//!   start (the *live boundary*); events are dispatched with
//!   [`DispatchContext::is_catching_up`] = `true`.
//! * **Live** subscribes from the next position; events after the boundary
//!   are dispatched with `is_catching_up = false`.
//! * **At-least-once delivery, exactly-once effect**: every event at or below
//!   the committed position is skipped, so duplicate delivery (subscription
//!   overlap, retries, restarts) is harmless.
//! * **Ordering requirement**: the event store must deliver a tenant's live
//!   events in global-nonce commit order (the `commit_ordered_global_nonce`
//!   capability, see #366). A live event below the last applied position
//!   that is not provably a duplicate stops the runner with
//!   [`Error::OutOfOrderDelivery`] rather than being skipped and lost.
//! * **Feed prefixes** are matched literally, including `\`, `%` and `_`.
//!   Live delivery relies on the server doing the same (the
//!   `literal_subscription_prefix` capability, #361); servers before v0.17.0
//!   used the prefix as an unescaped SQL `LIKE` pattern on Postgres, where a
//!   prefix containing `\` can miss events. Check the capability with
//!   `GetServerInfo` before using such a prefix against an unknown server.
//! * **Atomicity**: for each event the runner calls
//!   [`ProjectionStore::begin`], [`CheckpointedProjection::handle`], then
//!   [`ProjectionStore::commit`] with the event's position. A transactional
//!   store commits the projection's writes and the checkpoint in one
//!   transaction ([`InMemoryProjectionStore`], and `PostgresProjectionStore`
//!   behind the `postgres` feature). A handler error drops the transaction
//!   and leaves the checkpoint where it was.
//! * **External stores** (search indexes, vector DBs) cannot share that
//!   transaction. Use [`ExternalCheckpoints`]: the handler writes to the
//!   external store directly and the checkpoint is saved afterwards. A crash
//!   between the two redelivers the event on restart, so the handler **must**
//!   be idempotent (upsert keyed by event id or aggregate id + nonce).
//! * **Errors propagate**: a failing handler, a failing commit, a failing or
//!   ended subscription stream stops the runner with an error. The runner
//!   never skips an event it failed to process.
//! * **Cancellation** is cooperative through a `CancellationToken` and takes
//!   effect between events; an in-flight event is finished or rolled back,
//!   never half-committed.
//! * **Rebuild**: [`ProjectionRunner::rebuild`] resets the projection's data
//!   and checkpoint for its key only. Bumping the version gives a new key, so
//!   a new version builds from zero next to the old one.
//! * **No side effects during replay**: projections must be pure. Side
//!   effects belong to a [`LiveProcessor`] (the processor half of a process
//!   manager): the projection writes to-do records in `handle`, and the
//!   runner wakes the processor only after committing a *live* event, never
//!   while catching up.

use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use tokio::sync::{watch, Notify};
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::client::{proto, EventStorePort};
use crate::error::{Error, Result};

#[cfg(feature = "postgres")]
mod postgres;
#[cfg(feature = "postgres")]
pub use postgres::PostgresProjectionStore;

/// Identity of a projection's position in a feed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CheckpointKey {
    /// Tenant whose global log is consumed.
    pub tenant_id: String,
    /// Projection name.
    pub projection_name: String,
    /// Projection schema version. A new version is a new feed position.
    pub projection_version: u32,
    /// Feed filter: aggregate-id prefix (`""` = the whole tenant log).
    pub feed: String,
}

impl CheckpointKey {
    /// Key for a projection over a tenant's whole log.
    pub fn new(
        tenant_id: impl Into<String>,
        projection_name: impl Into<String>,
        projection_version: u32,
    ) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            projection_name: projection_name.into(),
            projection_version,
            feed: String::new(),
        }
    }

    /// Restrict the feed to aggregate ids starting with `prefix`.
    pub fn with_feed(mut self, prefix: impl Into<String>) -> Self {
        self.feed = prefix.into();
        self
    }
}

impl fmt::Display for CheckpointKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}/{}@v{}[{}]",
            self.tenant_id, self.projection_name, self.projection_version, self.feed
        )
    }
}

/// An event as recorded in the store, with an untyped payload.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedEvent {
    pub event_id: String,
    pub event_type: String,
    pub event_version: u32,
    pub tenant_id: String,
    pub aggregate_id: String,
    pub aggregate_type: String,
    pub aggregate_nonce: u64,
    pub global_nonce: u64,
    pub content_type: String,
    pub correlation_id: Option<String>,
    pub causation_id: Option<String>,
    pub actor_id: Option<String>,
    pub timestamp_unix_ms: u64,
    pub recorded_time_unix_ms: u64,
    pub headers: HashMap<String, String>,
    pub payload: Vec<u8>,
}

fn non_empty(s: String) -> Option<String> {
    (!s.is_empty()).then_some(s)
}

impl RecordedEvent {
    /// Deserialize the JSON payload.
    pub fn decode<T: DeserializeOwned>(&self) -> Result<T> {
        Ok(serde_json::from_slice(&self.payload)?)
    }

    /// Convert a wire event. Fails if metadata is missing.
    pub fn from_proto(data: proto::EventData) -> Result<Self> {
        let m = data
            .meta
            .ok_or_else(|| Error::Repository(anyhow::anyhow!("event without metadata in feed")))?;
        Ok(Self {
            event_id: m.event_id,
            event_type: m.event_type,
            event_version: m.event_version,
            tenant_id: m.tenant_id,
            aggregate_id: m.aggregate_id,
            aggregate_type: m.aggregate_type,
            aggregate_nonce: m.aggregate_nonce,
            global_nonce: m.global_nonce,
            content_type: m.content_type,
            correlation_id: non_empty(m.correlation_id),
            causation_id: non_empty(m.causation_id),
            actor_id: non_empty(m.actor_id),
            timestamp_unix_ms: m.timestamp_unix_ms,
            recorded_time_unix_ms: m.recorded_time_unix_ms,
            headers: m.headers,
            payload: data.payload,
        })
    }
}

/// Replay awareness passed to every `handle` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DispatchContext {
    /// True while replaying history (at or below the live boundary).
    pub is_catching_up: bool,
    /// Position of the current event.
    pub global_nonce: u64,
    /// Head of the tenant log observed when the runner started.
    pub live_boundary_nonce: u64,
}

/// Storage for a projection's checkpoint, and (for transactional stores) its
/// read model.
#[async_trait]
pub trait ProjectionStore: Send + Sync {
    /// Unit of work for one event. Dropping it without `commit` discards the
    /// projection's writes.
    type Tx: Send;

    /// Last committed position for `key`, if any.
    async fn load_checkpoint(&self, key: &CheckpointKey) -> Result<Option<u64>>;

    /// Start a unit of work for one event of `key`.
    async fn begin(&self, key: &CheckpointKey) -> Result<Self::Tx>;

    /// Commit the projection's writes in `tx` and set the checkpoint of `key`
    /// to `position`, atomically when the store is transactional.
    ///
    /// Must fail (and commit nothing) if the stored checkpoint is already at
    /// or beyond `position`; this fences off a second runner on the same key.
    async fn commit(&self, tx: Self::Tx, key: &CheckpointKey, position: u64) -> Result<()>;

    /// Remove the checkpoint of `key`.
    async fn delete_checkpoint(&self, key: &CheckpointKey) -> Result<()>;

    /// Start a rebuild of `key`; the projection clears its data through the
    /// returned unit of work, then [`commit_reset`](Self::commit_reset) runs.
    ///
    /// Default (non-transactional stores): delete the checkpoint *first*, so
    /// a crash mid-reset replays from zero into idempotent handlers rather
    /// than resuming over cleared data. Transactional stores override both
    /// methods to clear data and checkpoint in one transaction.
    async fn begin_reset(&self, key: &CheckpointKey) -> Result<Self::Tx> {
        self.delete_checkpoint(key).await?;
        self.begin(key).await
    }

    /// Finish a rebuild: commit the projection's reset writes with the
    /// checkpoint of `key` removed. Default: nothing left to commit.
    async fn commit_reset(&self, tx: Self::Tx, key: &CheckpointKey) -> Result<()> {
        let _ = (tx, key);
        Ok(())
    }
}

/// A read model built from events, with mandatory checkpointing.
///
/// `handle` must be deterministic and free of external side effects: it runs
/// for every historical event on every rebuild. Write to-do records for a
/// [`LiveProcessor`] instead of performing side effects.
#[async_trait]
pub trait CheckpointedProjection<S: ProjectionStore>: Send {
    /// Stable projection name.
    fn name(&self) -> &str;

    /// Schema version. Bump to build a new read model from zero.
    fn version(&self) -> u32;

    /// Whether this projection wants `event_type`. Unwanted events still
    /// advance the checkpoint. Defaults to all events.
    fn handles(&self, event_type: &str) -> bool {
        let _ = event_type;
        true
    }

    /// Apply one event, writing through `tx`.
    async fn handle(
        &mut self,
        tx: &mut S::Tx,
        event: &RecordedEvent,
        ctx: &DispatchContext,
    ) -> Result<()>;

    /// Delete all read-model data for `key` (rebuild), writing through `tx`.
    /// For transactional stores this commits atomically with the checkpoint
    /// removal (see [`ProjectionStore::begin_reset`]).
    async fn reset(&mut self, tx: &mut S::Tx, key: &CheckpointKey) -> Result<()>;
}

/// Processor half of a process manager: executes pending to-do items with
/// side effects. Must be idempotent; it is only woken by live events.
#[async_trait]
pub trait LiveProcessor: Send + Sync {
    /// Execute pending items; return how many were processed.
    async fn process_pending(&self) -> Result<usize>;
}

// ---------------------------------------------------------------------------
// Checkpoint-only stores (external read models)
// ---------------------------------------------------------------------------

/// Checkpoint persistence without a shared transaction.
#[async_trait]
pub trait CheckpointStore: Send + Sync {
    /// Last saved position for `key`.
    async fn load(&self, key: &CheckpointKey) -> Result<Option<u64>>;
    /// Save `position` for `key`; fails if the stored position is already at
    /// or beyond it.
    async fn save(&self, key: &CheckpointKey, position: u64) -> Result<()>;
    /// Remove the checkpoint of `key`.
    async fn delete(&self, key: &CheckpointKey) -> Result<()>;
}

/// [`ProjectionStore`] for read models that live outside the checkpoint's
/// transaction. `Tx` is `()`: the handler writes to its external store
/// directly, then the checkpoint is saved. Handlers must be idempotent.
pub struct ExternalCheckpoints<C> {
    checkpoints: C,
}

impl<C: CheckpointStore> ExternalCheckpoints<C> {
    /// Wrap a checkpoint store.
    pub fn new(checkpoints: C) -> Self {
        Self { checkpoints }
    }

    /// The wrapped checkpoint store.
    pub fn checkpoints(&self) -> &C {
        &self.checkpoints
    }
}

#[async_trait]
impl<C: CheckpointStore> ProjectionStore for ExternalCheckpoints<C> {
    type Tx = ();

    async fn load_checkpoint(&self, key: &CheckpointKey) -> Result<Option<u64>> {
        self.checkpoints.load(key).await
    }

    async fn begin(&self, _key: &CheckpointKey) -> Result<()> {
        Ok(())
    }

    async fn commit(&self, _tx: (), key: &CheckpointKey, position: u64) -> Result<()> {
        self.checkpoints.save(key, position).await
    }

    async fn delete_checkpoint(&self, key: &CheckpointKey) -> Result<()> {
        self.checkpoints.delete(key).await
    }
}

fn fence_error(key: &CheckpointKey, stored: u64, position: u64) -> Error {
    Error::Repository(anyhow::anyhow!(
        "checkpoint for {key} is at {stored}; refusing to commit {position} \
         (another runner owns this key, or the event was already processed)"
    ))
}

/// In-memory [`CheckpointStore`] for tests and single-process use.
#[derive(Debug, Default)]
pub struct InMemoryCheckpointStore {
    positions: Mutex<HashMap<CheckpointKey, u64>>,
}

impl InMemoryCheckpointStore {
    /// Empty store.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl CheckpointStore for InMemoryCheckpointStore {
    async fn load(&self, key: &CheckpointKey) -> Result<Option<u64>> {
        Ok(self.positions.lock().expect("poisoned").get(key).copied())
    }

    async fn save(&self, key: &CheckpointKey, position: u64) -> Result<()> {
        let mut positions = self.positions.lock().expect("poisoned");
        if let Some(&stored) = positions.get(key) {
            if stored >= position {
                return Err(fence_error(key, stored, position));
            }
        }
        positions.insert(key.clone(), position);
        Ok(())
    }

    async fn delete(&self, key: &CheckpointKey) -> Result<()> {
        self.positions.lock().expect("poisoned").remove(key);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// In-memory transactional store
// ---------------------------------------------------------------------------

/// Transactional in-memory read model + checkpoint, one `S` per key.
///
/// `begin` stages a copy of the key's state; `commit` swaps state and
/// checkpoint in under one lock. Intended for tests, examples, and small
/// single-process read models (each event clones the key's state).
#[derive(Debug)]
pub struct InMemoryProjectionStore<S> {
    slots: Mutex<HashMap<CheckpointKey, (S, Option<u64>)>>,
}

impl<S> Default for InMemoryProjectionStore<S> {
    fn default() -> Self {
        Self {
            slots: Mutex::new(HashMap::new()),
        }
    }
}

/// Staged unit of work of an [`InMemoryProjectionStore`].
#[derive(Debug)]
pub struct InMemoryTx<S> {
    /// Working copy of the key's state; mutate it in `handle`.
    pub state: S,
}

impl<S: Clone + Default + Send + Sync> InMemoryProjectionStore<S> {
    /// Empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Committed state of `key` (default if never written).
    pub fn state(&self, key: &CheckpointKey) -> S {
        self.slots
            .lock()
            .expect("poisoned")
            .get(key)
            .map(|(s, _)| s.clone())
            .unwrap_or_default()
    }
}

#[async_trait]
impl<S: Clone + Default + Send + Sync> ProjectionStore for InMemoryProjectionStore<S> {
    type Tx = InMemoryTx<S>;

    async fn load_checkpoint(&self, key: &CheckpointKey) -> Result<Option<u64>> {
        Ok(self
            .slots
            .lock()
            .expect("poisoned")
            .get(key)
            .and_then(|(_, p)| *p))
    }

    async fn begin(&self, key: &CheckpointKey) -> Result<InMemoryTx<S>> {
        Ok(InMemoryTx {
            state: self.state(key),
        })
    }

    async fn commit(&self, tx: InMemoryTx<S>, key: &CheckpointKey, position: u64) -> Result<()> {
        let mut slots = self.slots.lock().expect("poisoned");
        let slot = slots
            .entry(key.clone())
            .or_insert_with(|| (S::default(), None));
        if let Some(stored) = slot.1 {
            if stored >= position {
                return Err(fence_error(key, stored, position));
            }
        }
        *slot = (tx.state, Some(position));
        Ok(())
    }

    async fn delete_checkpoint(&self, key: &CheckpointKey) -> Result<()> {
        if let Some(slot) = self.slots.lock().expect("poisoned").get_mut(key) {
            slot.1 = None;
        }
        Ok(())
    }

    async fn begin_reset(&self, key: &CheckpointKey) -> Result<InMemoryTx<S>> {
        self.begin(key).await
    }

    async fn commit_reset(&self, tx: InMemoryTx<S>, key: &CheckpointKey) -> Result<()> {
        self.slots
            .lock()
            .expect("poisoned")
            .insert(key.clone(), (tx.state, None));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Runner
// ---------------------------------------------------------------------------

/// Observable progress of a runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RunnerProgress {
    /// Last committed position (0 = nothing processed).
    pub position: u64,
    /// True once catch-up finished and the live subscription is open.
    pub is_live: bool,
}

/// Why [`ProjectionRunner::run`] returned without error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunExit {
    /// The cancellation token fired.
    Cancelled {
        /// Last committed position.
        position: u64,
    },
}

const DEFAULT_PAGE_SIZE: u32 = 500;
/// `read_all` backward from here returns the tenant head. Postgres binds
/// positions as `i64`, so stay within its range.
const HEAD_PROBE: u64 = i64::MAX as u64;

/// Drives one checkpointed projection over one tenant feed.
pub struct ProjectionRunner<P, S: ProjectionStore> {
    events: Arc<dyn EventStorePort>,
    store: Arc<S>,
    projection: P,
    key: CheckpointKey,
    page_size: u32,
    processor: Option<Arc<dyn LiveProcessor>>,
    drain_on_live_start: bool,
    processor_retry: Duration,
    position: u64,
    progress: watch::Sender<RunnerProgress>,
}

impl<P, S> ProjectionRunner<P, S>
where
    P: CheckpointedProjection<S>,
    S: ProjectionStore,
{
    /// Runner for `projection` over the whole log of `tenant_id`.
    pub fn new(
        events: Arc<dyn EventStorePort>,
        store: Arc<S>,
        projection: P,
        tenant_id: impl Into<String>,
    ) -> Self {
        let key = CheckpointKey::new(tenant_id, projection.name(), projection.version());
        let (progress, _) = watch::channel(RunnerProgress::default());
        Self {
            events,
            store,
            projection,
            key,
            page_size: DEFAULT_PAGE_SIZE,
            processor: None,
            drain_on_live_start: true,
            processor_retry: Duration::from_secs(1),
            position: 0,
            progress,
        }
    }

    /// Only consume aggregates whose id starts with `prefix`. Part of the
    /// checkpoint identity.
    pub fn with_feed_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.key.feed = prefix.into();
        self
    }

    /// Page size for catch-up reads.
    pub fn with_page_size(mut self, page_size: u32) -> Self {
        self.page_size = page_size.clamp(1, 1000);
        self
    }

    /// Wake `processor` after each committed live event (never during
    /// catch-up). It runs on its own task, one pass at a time; wake-ups
    /// during a pass coalesce into exactly one more pass. A failed pass is
    /// retried with backoff until it succeeds or the runner stops.
    pub fn with_live_processor(mut self, processor: Arc<dyn LiveProcessor>) -> Self {
        self.processor = Some(processor);
        self
    }

    /// Run one processor pass when the runner goes live, to resume to-do
    /// items left pending by a crash or an earlier failed pass. The pass runs
    /// after catch-up has finished, never during replay. On by default: a
    /// crash after committing a live event but before its pass would
    /// otherwise strand the item until the next live event. Requires
    /// `process_pending` to be idempotent (durable dedup), as ADR-025 does.
    pub fn drain_pending_on_live_start(mut self, enabled: bool) -> Self {
        self.drain_on_live_start = enabled;
        self
    }

    /// Delay before retrying a failed processor pass (doubles per failure,
    /// capped at 30 s). Default 1 s.
    pub fn with_processor_retry_delay(mut self, delay: Duration) -> Self {
        self.processor_retry = delay;
        self
    }

    /// Checkpoint identity of this runner.
    pub fn key(&self) -> &CheckpointKey {
        &self.key
    }

    /// The projection.
    pub fn projection(&self) -> &P {
        &self.projection
    }

    /// The projection, mutably.
    pub fn projection_mut(&mut self) -> &mut P {
        &mut self.projection
    }

    /// Last committed position known to this runner.
    pub fn position(&self) -> u64 {
        self.position
    }

    /// Progress updates (position, live flag).
    pub fn progress(&self) -> watch::Receiver<RunnerProgress> {
        self.progress.subscribe()
    }

    /// Reset this key's read model and checkpoint. The next `catch_up` or
    /// `run` replays the feed from the beginning. Other keys (tenants,
    /// projections, versions) are untouched.
    ///
    /// Transactional stores clear data and checkpoint atomically. Stop any
    /// other runner on this key first: an empty checkpoint cannot fence it.
    pub async fn rebuild(&mut self) -> Result<()> {
        let mut tx = self.store.begin_reset(&self.key).await?;
        self.projection.reset(&mut tx, &self.key).await?;
        self.store.commit_reset(tx, &self.key).await?;
        self.position = 0;
        self.publish(false);
        Ok(())
    }

    /// Process every event up to the current head, then return the
    /// committed position. Does not subscribe.
    pub async fn catch_up(&mut self) -> Result<u64> {
        self.catch_up_until(&CancellationToken::new()).await?;
        Ok(self.position)
    }

    /// Catch up, then consume live events until `cancel` fires or an error
    /// occurs. Errors are returned, never swallowed; the checkpoint stays at
    /// the last event that committed.
    pub async fn run(&mut self, cancel: CancellationToken) -> Result<RunExit> {
        let Some(boundary) = self.catch_up_until(&cancel).await? else {
            return Ok(RunExit::Cancelled {
                position: self.position,
            });
        };

        let mut stream = self
            .events
            .subscribe(proto::SubscribeRequest {
                tenant_id: self.key.tenant_id.clone(),
                aggregate_id_prefix: self.key.feed.clone(),
                from_global_nonce: self.position + 1,
            })
            .await?;
        self.publish(true);

        let drain = self.processor.clone().map(|p| {
            let drain = Drain::spawn(p, cancel.child_token(), self.processor_retry);
            if self.drain_on_live_start {
                drain.wake();
            }
            drain
        });
        let result = loop {
            let item = tokio::select! {
                biased;
                _ = cancel.cancelled() => break Ok(RunExit::Cancelled { position: self.position }),
                item = stream.next() => item,
            };
            let data = match item {
                Some(Ok(data)) => data,
                Some(Err(err)) => break Err(err),
                None => {
                    break Err(Error::from(tonic::Status::unavailable(
                        "subscription stream ended",
                    )))
                }
            };
            let event = match RecordedEvent::from_proto(data) {
                Ok(event) => event,
                Err(err) => break Err(err),
            };
            // Servers before `literal_subscription_prefix` (#361) may match
            // the feed prefix loosely (unescaped SQL LIKE); enforce exact
            // prefix semantics so foreign events never reach the projection
            // or advance its checkpoint.
            if !event.aggregate_id.starts_with(&self.key.feed) {
                continue;
            }
            // Live delivery must be in commit (global nonce) order. A nonce
            // above the catch-up boundary but below the last applied one is
            // either a reordered event never seen (skipping it would lose it
            // silently) or a late duplicate; the two cannot be told apart,
            // so fail loudly instead of guessing. The last applied nonce
            // itself is a provable duplicate and is skipped.
            if event.global_nonce > boundary && event.global_nonce < self.position {
                break Err(Error::OutOfOrderDelivery {
                    projection: self.key.to_string(),
                    last_applied: self.position,
                    received: event.global_nonce,
                });
            }
            let ctx = DispatchContext {
                is_catching_up: event.global_nonce <= boundary,
                global_nonce: event.global_nonce,
                live_boundary_nonce: boundary,
            };
            match self.process(&event, &ctx).await {
                Ok(true) if !ctx.is_catching_up => {
                    if let Some(drain) = &drain {
                        drain.wake();
                    }
                }
                Ok(_) => {}
                Err(err) => break Err(err),
            }
        };
        self.publish(false);
        if let Some(drain) = drain {
            drain.stop().await;
        }
        result
    }

    /// Returns the live boundary, or `None` if cancelled.
    async fn catch_up_until(&mut self, cancel: &CancellationToken) -> Result<Option<u64>> {
        self.position = self.store.load_checkpoint(&self.key).await?.unwrap_or(0);
        self.publish(false);
        let boundary = self.head().await?;

        let mut from = self.position + 1;
        while from <= boundary {
            if cancel.is_cancelled() {
                return Ok(None);
            }
            let page = self
                .events
                .read_all(proto::ReadAllRequest {
                    tenant_id: self.key.tenant_id.clone(),
                    from_global_nonce: from,
                    max_count: self.page_size,
                    forward: true,
                })
                .await?;
            let Some(last) = page.events.last().and_then(|e| e.meta.as_ref()) else {
                break;
            };
            let next = last.global_nonce + 1;
            for data in page.events {
                if cancel.is_cancelled() {
                    return Ok(None);
                }
                let event = RecordedEvent::from_proto(data)?;
                if event.global_nonce > boundary {
                    break;
                }
                if !event.aggregate_id.starts_with(&self.key.feed) {
                    continue;
                }
                let ctx = DispatchContext {
                    is_catching_up: true,
                    global_nonce: event.global_nonce,
                    live_boundary_nonce: boundary,
                };
                self.process(&event, &ctx).await?;
            }
            if page.is_end || next <= from {
                break;
            }
            from = next;
        }
        Ok(Some(boundary))
    }

    async fn head(&self) -> Result<u64> {
        let page = self
            .events
            .read_all(proto::ReadAllRequest {
                tenant_id: self.key.tenant_id.clone(),
                from_global_nonce: HEAD_PROBE,
                max_count: 1,
                forward: false,
            })
            .await?;
        Ok(page
            .events
            .first()
            .and_then(|e| e.meta.as_ref())
            .map(|m| m.global_nonce)
            .unwrap_or(0))
    }

    /// Handle and commit one event. Returns false for a duplicate.
    async fn process(&mut self, event: &RecordedEvent, ctx: &DispatchContext) -> Result<bool> {
        if event.global_nonce <= self.position {
            return Ok(false);
        }
        let mut tx = self.store.begin(&self.key).await?;
        if self.projection.handles(&event.event_type) {
            self.projection
                .handle(&mut tx, event, ctx)
                .await
                .map_err(|source| Error::ProjectionFailed {
                    projection: self.key.to_string(),
                    global_nonce: event.global_nonce,
                    source: Box::new(source),
                })?;
        }
        self.store.commit(tx, &self.key, event.global_nonce).await?;
        self.position = event.global_nonce;
        self.publish(!ctx.is_catching_up);
        Ok(true)
    }

    fn publish(&self, is_live: bool) {
        self.progress.send_replace(RunnerProgress {
            position: self.position,
            is_live,
        });
    }
}

/// Background task running a [`LiveProcessor`] one pass at a time.
struct Drain {
    wake: Arc<Notify>,
    stop: CancellationToken,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl Drain {
    fn spawn(processor: Arc<dyn LiveProcessor>, stop: CancellationToken, retry: Duration) -> Self {
        const MAX_RETRY: Duration = Duration::from_secs(30);
        let wake = Arc::new(Notify::new());
        let task_wake = wake.clone();
        let task_stop = stop.clone();
        let handle = tokio::spawn(async move {
            // Some(delay) while the last pass failed: retry after `delay`
            // even if no new live event arrives.
            let mut retry_after: Option<Duration> = None;
            loop {
                match retry_after {
                    None => tokio::select! {
                        biased;
                        _ = task_stop.cancelled() => return,
                        _ = task_wake.notified() => {}
                    },
                    Some(delay) => tokio::select! {
                        biased;
                        _ = task_stop.cancelled() => return,
                        _ = task_wake.notified() => {}
                        _ = tokio::time::sleep(delay) => {}
                    },
                }
                match processor.process_pending().await {
                    Ok(_) => retry_after = None,
                    Err(err) => {
                        // Pending items stay pending and are retried.
                        tracing::warn!(error = %err, "live processor pass failed");
                        retry_after = Some(match retry_after {
                            None => retry,
                            Some(d) => (d * 2).min(MAX_RETRY),
                        });
                    }
                }
            }
        });
        Self {
            wake,
            stop,
            handle: Some(handle),
        }
    }

    fn wake(&self) {
        // Stores one permit: wake-ups during a pass coalesce into one more.
        self.wake.notify_one();
    }

    /// Let the current pass finish, then stop.
    async fn stop(mut self) {
        self.stop.cancel();
        // Await through a reference: if this future is dropped mid-wait
        // (run() aborted during shutdown), `Drain` still owns the handle and
        // `Drop` aborts the task instead of detaching it.
        if let Some(handle) = self.handle.as_mut() {
            let _ = handle.await;
        }
        self.handle = None;
    }
}

impl Drop for Drain {
    /// `run()` was dropped or aborted without a graceful stop: the processor
    /// must not outlive its runner. Cancel and abort the task (an interrupted
    /// pass is safe because `process_pending` is idempotent).
    fn drop(&mut self) {
        self.stop.cancel();
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str) -> CheckpointKey {
        CheckpointKey::new("t", name, 1)
    }

    #[tokio::test]
    async fn in_memory_store_commits_state_and_checkpoint_together() {
        let store = InMemoryProjectionStore::<Vec<u64>>::new();
        let k = key("p");
        let mut tx = store.begin(&k).await.unwrap();
        tx.state.push(1);
        // Dropped without commit: nothing visible.
        drop(tx);
        assert!(store.state(&k).is_empty());
        assert_eq!(store.load_checkpoint(&k).await.unwrap(), None);

        let mut tx = store.begin(&k).await.unwrap();
        tx.state.push(5);
        store.commit(tx, &k, 5).await.unwrap();
        assert_eq!(store.state(&k), vec![5]);
        assert_eq!(store.load_checkpoint(&k).await.unwrap(), Some(5));
    }

    #[tokio::test]
    async fn stores_fence_non_advancing_commits() {
        let store = InMemoryProjectionStore::<Vec<u64>>::new();
        let k = key("p");
        store
            .commit(InMemoryTx { state: vec![1] }, &k, 3)
            .await
            .unwrap();
        let err = store
            .commit(InMemoryTx { state: vec![9] }, &k, 3)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("refusing"), "{err}");
        assert_eq!(store.state(&k), vec![1], "fenced commit applied nothing");

        let cps = InMemoryCheckpointStore::new();
        cps.save(&k, 2).await.unwrap();
        assert!(cps.save(&k, 2).await.is_err());
        assert!(cps.save(&k, 1).await.is_err());
        cps.save(&k, 4).await.unwrap();
        cps.delete(&k).await.unwrap();
        assert_eq!(cps.load(&k).await.unwrap(), None);
    }

    #[test]
    fn checkpoint_identity_includes_tenant_version_and_feed() {
        let base = CheckpointKey::new("a", "p", 1);
        assert_ne!(base, CheckpointKey::new("b", "p", 1));
        assert_ne!(base, CheckpointKey::new("a", "p", 2));
        assert_ne!(base, CheckpointKey::new("a", "q", 1));
        assert_ne!(base, base.clone().with_feed("order-"));
        assert_eq!(base.to_string(), "a/p@v1[]");
    }
}
