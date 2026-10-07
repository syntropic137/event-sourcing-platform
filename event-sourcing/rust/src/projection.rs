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
//!   prefix containing `\` can miss events. With the capability guard on
//!   (default), a feed containing `\` also requires that capability.
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
//!   ended subscription stream stops [`ProjectionRunner::run`] with an error.
//!   The runner never skips an event it failed to process.
//! * **Supervision**: [`ProjectionRunner::run_supervised`] reconnects from the
//!   checkpoint after transient failures (jittered [`BackoffPolicy`]), halts
//!   at an undecodable stored event with [`Error::DataLoss`] instead of
//!   skipping or retrying it (ADR-026), and stops with typed errors on
//!   everything else. [`ProjectionRunner::health`] reports state, position,
//!   lag, halt and last error.
//! * **Capability guard**: `run` and `catch_up` first require the event
//!   store to advertise [`REQUIRED_CAPABILITIES`] (opt out with
//!   [`ProjectionRunner::without_capability_check`]).
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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use tokio::sync::watch;
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::client::{capabilities, proto, CompatibilityError, EventStorePort};
use crate::error::{Error, Result};
use crate::event::{DomainEvent, SerializedEvent};
use crate::upcast::Upcasters;
use crate::wire;

mod drain;
#[cfg(feature = "postgres")]
mod postgres;
mod supervisor;
use drain::Drain;
pub use drain::ProcessorPanicPolicy;
#[cfg(feature = "postgres")]
pub use postgres::PostgresProjectionStore;
pub use supervisor::{BackoffPolicy, RunnerHealth, RunnerState};

/// Capabilities a [`ProjectionRunner`] requires of the event store by
/// default (see [`ProjectionRunner::with_required_capabilities`]):
///
/// * `commit_ordered_global_nonce`: live events arrive in commit order, so
///   skipping duplicates by position never drops an event (#366).
/// * `subscription_errors_surfaced`: a failed backend query ends the stream
///   with `UNAVAILABLE` instead of looking like "no new events" (ADR-026).
/// * `undecodable_events_surfaced`: an undecodable stored event ends the
///   stream with `DATA_LOSS` instead of being skipped (ADR-026).
pub const REQUIRED_CAPABILITIES: [&str; 3] = [
    capabilities::COMMIT_ORDERED_GLOBAL_NONCE,
    capabilities::SUBSCRIPTION_ERRORS_SURFACED,
    capabilities::UNDECODABLE_EVENTS_SURFACED,
];

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
///
/// Decode it with [`decode`](Self::decode), which dispatches on
/// `event_type` and `event_version` (ADR-027). Inside a
/// [`ProjectionRunner`] configured with
/// [`with_upcasters`](ProjectionRunner::with_upcasters), the event has already
/// been upcast: type, version and payload are the chain's result.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedEvent {
    pub event_id: String,
    pub event_type: String,
    /// Schema version (`0` on the wire, meaning unset, is reported as 1).
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
    /// Decode as `E` by dispatching on `event_type` and `event_version`.
    ///
    /// An event `E` does not know is [`Error::UnknownEventType`] or
    /// [`Error::UnknownEventVersion`]; a non-JSON payload is
    /// [`Error::UnsupportedContentType`]. `E` is an
    /// [`event_enum!`](crate::event_enum) or a single
    /// [`EventSchema`](crate::event::EventSchema) struct.
    pub fn decode<E: DomainEvent>(&self) -> Result<E> {
        wire::check_content_type(&self.event_type, &self.content_type)?;
        E::from_payload(&SerializedEvent::new(
            &self.event_type,
            self.event_version,
            &self.payload,
        ))
    }

    /// Upcast with `upcasters`, then [`decode`](Self::decode). For events
    /// obtained outside a runner configured with upcasters.
    pub fn decode_with<E: DomainEvent>(&self, upcasters: &Upcasters) -> Result<E> {
        upcasters.upcast_recorded(self)?.decode()
    }

    /// Deserialize the raw JSON payload as `T`, without checking type or
    /// version and without upcasting. Prefer [`decode`](Self::decode).
    pub fn payload_json<T: DeserializeOwned>(&self) -> Result<T> {
        wire::check_content_type(&self.event_type, &self.content_type)?;
        SerializedEvent::new(&self.event_type, self.event_version, &self.payload).deserialize()
    }

    /// Convert a wire event. Fails if metadata is missing.
    pub fn from_proto(data: proto::EventData) -> Result<Self> {
        let m = data
            .meta
            .ok_or_else(|| Error::Repository(anyhow::anyhow!("event without metadata in feed")))?;
        Ok(Self {
            event_id: m.event_id,
            event_type: m.event_type,
            event_version: wire::normalize_version(m.event_version),
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
    Error::CheckpointFenced {
        projection: key.to_string(),
        stored,
        position,
    }
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
    processor_shutdown_grace: Duration,
    upcasters: Upcasters,
    position: u64,
    progress: watch::Sender<RunnerProgress>,
    required_capabilities: Vec<String>,
    panic_policy: ProcessorPanicPolicy,
    undecodable_recheck: Option<Duration>,
    /// Position of the undecodable event the runner is halted at.
    halted_at: Option<u64>,
    /// Holds live processor passes while halted.
    paused: Arc<AtomicBool>,
    health: watch::Sender<RunnerHealth>,
    /// Checkpoint loaded by the current attempt (progress detection).
    loaded_position: u64,
    /// When the current attempt went live.
    live_since: Option<Instant>,
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
            processor_shutdown_grace: Duration::from_secs(10),
            upcasters: Upcasters::new(),
            position: 0,
            progress,
            required_capabilities: REQUIRED_CAPABILITIES.map(String::from).to_vec(),
            panic_policy: ProcessorPanicPolicy::default(),
            undecodable_recheck: None,
            halted_at: None,
            paused: Arc::new(AtomicBool::new(false)),
            health: watch::channel(RunnerHealth::default()).0,
            loaded_position: 0,
            live_since: None,
        }
    }

    /// Capabilities the event store must advertise before the runner reads
    /// anything (checked at the start of every `run`, `catch_up` and
    /// supervised reconnect). Default: [`REQUIRED_CAPABILITIES`]. A server
    /// missing one (including a legacy server, or a custom
    /// [`EventStorePort`] that does not forward `server_info`) is refused
    /// with [`Error::Incompatible`], which is not retried.
    pub fn with_required_capabilities<I, C>(mut self, capabilities: I) -> Self
    where
        I: IntoIterator<Item = C>,
        C: Into<String>,
    {
        self.required_capabilities = capabilities.into_iter().map(Into::into).collect();
        self
    }

    /// Opt out of the capability guard. Only for event stores you have
    /// verified out of band: without `commit_ordered_global_nonce` live
    /// events can be skipped, and without the `*_surfaced` capabilities
    /// failures and undecodable events can be hidden (ADR-026).
    pub fn without_capability_check(mut self) -> Self {
        self.required_capabilities.clear();
        self
    }

    /// On cancellation, how long an in-flight [`LiveProcessor`] pass may run
    /// before it is cancelled (default 10 s), so a stalled external call
    /// cannot block shutdown. When the runner fails or halts, the pass is
    /// cancelled at once. `process_pending` is idempotent, so the next live
    /// pass redoes whatever was interrupted.
    pub fn with_processor_shutdown_grace(mut self, grace: Duration) -> Self {
        self.processor_shutdown_grace = grace;
        self
    }

    /// What to do when a [`LiveProcessor`] pass panics. Default
    /// [`ProcessorPanicPolicy::Stop`]: the runner stops with
    /// [`Error::LiveProcessorPanicked`].
    pub fn on_processor_panic(mut self, policy: ProcessorPanicPolicy) -> Self {
        self.panic_policy = policy;
        self
    }

    /// Under [`run_supervised`](Self::run_supervised), stay halted at an
    /// undecodable stored event (`DATA_LOSS`) and re-check every `interval`
    /// instead of returning [`Error::DataLoss`]. The runner resumes on its
    /// own once an operator repaired the row or moved the checkpoint past
    /// it (ADR-026). Intervals below 10 ms are raised to 10 ms.
    pub fn with_undecodable_recheck(mut self, interval: Duration) -> Self {
        self.undecodable_recheck = Some(interval.max(Duration::from_millis(10)));
        self
    }

    /// Health updates: state, position, live boundary, halt, last error,
    /// failures. Cheap to clone and poll from a readiness endpoint.
    pub fn health(&self) -> watch::Receiver<RunnerHealth> {
        self.health.subscribe()
    }

    /// Position of the undecodable stored event this runner is halted at.
    pub fn halted_at(&self) -> Option<u64> {
        self.halted_at
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

    /// Upcast every event before the projection sees it (and before
    /// [`CheckpointedProjection::handles`] is asked, so a renamed type is
    /// routed by its new name). An upcaster failure stops the runner with
    /// [`Error::ProjectionFailed`] at that event; the checkpoint does not
    /// advance past it.
    pub fn with_upcasters(mut self, upcasters: Upcasters) -> Self {
        self.upcasters = upcasters;
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
        let never = CancellationToken::new();
        let result = async {
            self.check_capabilities(&never).await?;
            self.catch_up_until(&never).await
        }
        .await;
        match result {
            Ok(_) => {
                self.set_state(RunnerState::Stopped);
                Ok(self.position)
            }
            Err(err) => {
                self.record_failure(&err);
                Err(err)
            }
        }
    }

    /// Catch up, then consume live events until `cancel` fires or an error
    /// occurs. Errors are returned, never swallowed; the checkpoint stays at
    /// the last event that committed.
    pub async fn run(&mut self, cancel: CancellationToken) -> Result<RunExit> {
        let result = self.run_attempt(&cancel).await;
        match &result {
            Ok(_) => self.set_state(RunnerState::Stopped),
            Err(err) => self.record_failure(err),
        }
        result
    }

    /// One attempt of [`run`](Self::run): capability check, catch-up, live.
    async fn run_attempt(&mut self, cancel: &CancellationToken) -> Result<RunExit> {
        self.loaded_position = self.position;
        self.live_since = None;
        if !self.check_capabilities(cancel).await? {
            return Ok(RunExit::Cancelled {
                position: self.position,
            });
        }
        let Some(boundary) = self.catch_up_until(cancel).await? else {
            return Ok(RunExit::Cancelled {
                position: self.position,
            });
        };

        let subscribe = self.events.subscribe(proto::SubscribeRequest {
            tenant_id: self.key.tenant_id.clone(),
            aggregate_id_prefix: self.key.feed.clone(),
            from_global_nonce: self.position + 1,
        });
        let Some(stream) = or_cancelled(cancel, subscribe).await else {
            return Ok(RunExit::Cancelled {
                position: self.position,
            });
        };
        let mut stream = stream?;
        self.publish(true);
        self.live_since = Some(Instant::now());
        self.health.send_modify(|h| {
            h.state = RunnerState::Live;
            h.consecutive_failures = 0;
        });

        let mut drain = self.processor.clone().map(|p| {
            let drain = Drain::spawn(
                p,
                cancel.child_token(),
                self.processor_retry,
                self.paused.clone(),
                self.panic_policy,
                self.key.to_string(),
            );
            if self.drain_on_live_start {
                drain.wake();
            }
            drain
        });
        let result = loop {
            let item = tokio::select! {
                biased;
                _ = cancel.cancelled() => break Ok(RunExit::Cancelled { position: self.position }),
                message = processor_panicked(&mut drain) => {
                    break Err(Error::LiveProcessorPanicked {
                        projection: self.key.to_string(),
                        message,
                    })
                }
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
            if self.observe(event.global_nonce) {
                // Halt cleared: wake the processor held while halted.
                if let Some(drain) = &drain {
                    drain.wake();
                }
            }
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
            // Cancelled: let the in-flight pass finish. Failed or halted:
            // cancel it now (Python #380 does the same), so a pass blocked on
            // an external service cannot delay the halt or keep a side
            // effect running past it. A processor failure racing with either
            // is still surfaced: a dead processor never goes unnoticed.
            let died = match result {
                Ok(_) => drain.stop(self.processor_shutdown_grace).await,
                Err(_) => drain.abort().await,
            };
            if let Some(message) = died {
                if !matches!(result, Err(Error::LiveProcessorPanicked { .. })) {
                    if let Err(other) = &result {
                        tracing::error!(
                            projection = %self.key,
                            error = %other,
                            "runner error superseded by a live processor panic"
                        );
                    }
                    return Err(Error::LiveProcessorPanicked {
                        projection: self.key.to_string(),
                        message,
                    });
                }
            }
        }
        result
    }

    /// Fail with [`Error::Incompatible`] unless the event store advertises
    /// every required capability.
    /// Returns false if cancelled first.
    async fn check_capabilities(&mut self, cancel: &CancellationToken) -> Result<bool> {
        if self.required_capabilities.is_empty() {
            return Ok(true);
        }
        self.set_state(RunnerState::Starting);
        let Some(info) = or_cancelled(cancel, self.events.server_info()).await else {
            return Ok(false);
        };
        let info = info?;
        let mut required: Vec<&str> = self
            .required_capabilities
            .iter()
            .map(String::as_str)
            .collect();
        // Older servers use the prefix as an unescaped SQL LIKE pattern, where
        // `\` escapes the next character and live events can be missed (#361).
        if self.key.feed.contains('\\') {
            required.push(capabilities::LITERAL_SUBSCRIPTION_PREFIX);
        }
        let missing = info.missing_capabilities(&required);
        if missing.is_empty() {
            return Ok(true);
        }
        Err(Error::Incompatible(
            CompatibilityError::MissingCapabilities {
                server_version: info.server_version,
                missing,
            },
        ))
    }

    /// Returns the live boundary, or `None` if cancelled.
    async fn catch_up_until(&mut self, cancel: &CancellationToken) -> Result<Option<u64>> {
        let Some(loaded) = or_cancelled(cancel, self.store.load_checkpoint(&self.key)).await else {
            return Ok(None);
        };
        self.position = loaded?.unwrap_or(0);
        self.loaded_position = self.position;
        self.publish(false);
        // An operator moved the checkpoint to (or past) the halt position.
        self.observe(self.position);
        let Some(boundary) = or_cancelled(cancel, self.head()).await else {
            return Ok(None);
        };
        let boundary = boundary?;
        self.health.send_modify(|h| {
            h.state = RunnerState::CatchingUp;
            h.live_boundary = Some(boundary);
        });

        let mut from = self.position + 1;
        // Page size in effect. Drops to 1 below an undecodable event, so the
        // valid events before it are applied and the checkpoint reaches the
        // event just before it (ADR-026: an operator skip then skips only
        // the undecodable event).
        let mut limit = self.page_size;
        let mut slow_until = 0;
        while from <= boundary {
            if cancel.is_cancelled() {
                return Ok(None);
            }
            if from > slow_until {
                limit = self.page_size;
            }
            let read = self.events.read_all(proto::ReadAllRequest {
                tenant_id: self.key.tenant_id.clone(),
                from_global_nonce: from,
                max_count: limit,
                forward: true,
            });
            let Some(read) = or_cancelled(cancel, read).await else {
                return Ok(None);
            };
            let page = match read {
                Ok(page) => page,
                Err(Error::DataLoss { global_nonce, .. }) if global_nonce > from && limit > 1 => {
                    limit = 1;
                    slow_until = global_nonce;
                    continue;
                }
                Err(err) => return Err(err),
            };
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
                self.observe(event.global_nonce);
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
        let page = match self
            .events
            .read_all(proto::ReadAllRequest {
                tenant_id: self.key.tenant_id.clone(),
                from_global_nonce: HEAD_PROBE,
                max_count: 1,
                forward: false,
            })
            .await
        {
            Ok(page) => page,
            // The probe reads only the highest row, so the undecodable event
            // is the head: its position is the boundary. A runner whose
            // checkpoint an operator moved past it resumes; one still before
            // it stops there when catch-up reaches it (ADR-026).
            Err(Error::DataLoss { global_nonce, .. }) => {
                if self.halted_at == Some(global_nonce) {
                    tracing::debug!(projection = %self.key, global_nonce, "head event is undecodable");
                } else {
                    tracing::warn!(
                        projection = %self.key,
                        global_nonce,
                        "head event is undecodable; using its position as the live boundary"
                    );
                }
                return Ok(global_nonce);
            }
            Err(err) => return Err(err),
        };
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
        let failed = |source: Error| Error::ProjectionFailed {
            projection: self.key.to_string(),
            global_nonce: event.global_nonce,
            source: Box::new(source),
        };
        let upcast = self.upcasters.upcast_recorded(event).map_err(failed)?;
        let event = upcast.as_ref();
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
        let position = self.position;
        let committed = position > self.loaded_position;
        self.health.send_if_modified(|h| {
            let changed = h.position != position || (committed && h.consecutive_failures != 0);
            h.position = position;
            if committed {
                h.consecutive_failures = 0;
            }
            changed
        });
    }

    fn set_state(&self, state: RunnerState) {
        self.health.send_if_modified(|h| {
            let changed = h.state != state;
            h.state = state;
            changed
        });
    }

    /// Record a failed attempt in health; enter the halt on `DATA_LOSS`.
    fn record_failure(&mut self, err: &Error) {
        let message = err.to_string();
        self.health.send_modify(|h| {
            h.last_error = Some(message);
            h.consecutive_failures = h.consecutive_failures.saturating_add(1);
        });
        match err.data_loss_position() {
            Some(global_nonce) => self.enter_halt(global_nonce, err),
            None => self.set_state(RunnerState::Failed),
        }
    }

    /// Halt at an undecodable event: hold the live processor, report it, and
    /// log one `ERROR` per position (re-checks log at `DEBUG`).
    fn enter_halt(&mut self, global_nonce: u64, err: &Error) {
        if self.halted_at == Some(global_nonce) {
            tracing::debug!(projection = %self.key, global_nonce, "still halted at undecodable stored event");
        } else {
            tracing::error!(
                projection = %self.key,
                global_nonce,
                position = self.position,
                "{err}"
            );
        }
        self.halted_at = Some(global_nonce);
        self.paused.store(true, Ordering::SeqCst);
        self.health.send_modify(|h| {
            h.halted_at = Some(global_nonce);
            h.state = RunnerState::Halted { global_nonce };
        });
    }

    /// Note that the store delivered position `global_nonce` (or that the
    /// checkpoint is there). Clears the halt once the runner is at or past
    /// its position; returns true if it did.
    fn observe(&mut self, global_nonce: u64) -> bool {
        match self.halted_at {
            Some(halt) if global_nonce >= halt => {
                tracing::info!(
                    projection = %self.key,
                    global_nonce = halt,
                    "resumed past undecodable stored event"
                );
                self.halted_at = None;
                self.paused.store(false, Ordering::SeqCst);
                self.health.send_modify(|h| h.halted_at = None);
                true
            }
            _ => false,
        }
    }
}

/// Await `fut` unless `cancel` fires first. Only for calls that commit
/// nothing (reads, capability probe, opening a subscription), so dropping
/// them mid-flight is harmless.
async fn or_cancelled<T>(
    cancel: &CancellationToken,
    fut: impl std::future::Future<Output = T>,
) -> Option<T> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => None,
        value = fut => Some(value),
    }
}

/// Resolves when the live processor panicked (under
/// [`ProcessorPanicPolicy::Stop`]); never without a processor.
async fn processor_panicked(drain: &mut Option<Drain>) -> String {
    match drain {
        Some(drain) => drain.panicked().await,
        None => std::future::pending().await,
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
