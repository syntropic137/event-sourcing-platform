//! Repository pattern for loading and saving aggregates
//!
//! # Save semantics
//!
//! [`EventStoreRepository::save`] appends the instance's pending events as one
//! atomic batch with `expected_aggregate_nonce = committed_version()`:
//!
//! * **Success**: the store acknowledged the batch. Pending events are cleared
//!   and `committed_version()` advances to the new stream head.
//! * **Stale writer**: another writer advanced the stream first. The call
//!   returns [`Error::ConcurrencyConflict`] and **keeps** the pending events.
//!   The instance is stale; discard it, reload, and re-run the command. The
//!   repository never retries a conflict on its own, because the decision
//!   that produced the events was based on old state.
//! * **Unknown outcome** (transport failure, timeout, lost acknowledgment,
//!   see [`Error::is_transient`]): the store may or may not have committed.
//!   The repository retries according to its [`RetryPolicy`]. If retries are
//!   exhausted the error is returned and pending events are **kept**; calling
//!   `save` again on the same instance is safe.
//!
//! Retries are idempotent because a pending batch is immutable once recorded:
//! event IDs, timestamps, and nonces are fixed by
//! [`AggregateInstance::add_events`], and every attempt carries the same
//! idempotency key. The store checks the key before the concurrency
//! precondition (ADR-028), so a plain retry of a committed batch gets the
//! original ack directly. If a retry is still rejected as a conflict (or as
//! an idempotency key reuse), the repository reads the stream at the expected
//! position and compares event IDs. If its own batch is already there, the
//! save is treated as committed. If only a prefix is there (the caller
//! recorded more events after an unknown-outcome save, which changes the
//! key), the prefix is cleared and the remainder is appended on top of it.
//! The result is exactly-once in the stream.

use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};

use crate::aggregate::{Aggregate, AggregateInstance};
use crate::client::{proto, EventStorePort};
use crate::error::{Error, Result};
use crate::event::{DomainEvent, EventEnvelope};

/// Repository trait for loading and saving aggregates
#[async_trait]
pub trait Repository<A>: Send + Sync
where
    A: Aggregate,
{
    /// Load an aggregate by replaying its stream. `Ok(None)` if the stream
    /// does not exist.
    async fn load(&self, aggregate_id: &str) -> Result<Option<AggregateInstance<A>>>;

    /// Persist the instance's pending events (see module docs for semantics).
    async fn save(&self, instance: &mut AggregateInstance<A>) -> Result<()>;

    /// Check if an aggregate stream exists
    async fn exists(&self, aggregate_id: &str) -> Result<bool>;
}

/// Alias for the repository trait with clearer naming
pub type AggregateRepository<A> = dyn Repository<A>;

/// Retry policy for saves whose outcome is unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Total attempts including the first one (minimum 1).
    pub max_attempts: u32,
    /// Delay before the first retry; doubled after each retry.
    pub initial_backoff: Duration,
    /// Upper bound for the delay between retries.
    pub max_backoff: Duration,
}

impl RetryPolicy {
    /// Never retry automatically; the caller decides.
    pub fn none() -> Self {
        Self {
            max_attempts: 1,
            initial_backoff: Duration::ZERO,
            max_backoff: Duration::ZERO,
        }
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(50),
            max_backoff: Duration::from_secs(1),
        }
    }
}

const DEFAULT_PAGE_SIZE: u32 = 500;
const CONTENT_TYPE_JSON: &str = "application/json";

/// Repository backed by the event store through an [`EventStorePort`].
///
/// Streams are addressed by `(tenant_id, aggregate_id)`; aggregate IDs must be
/// unique within a tenant across aggregate types. Events are stored as JSON.
pub struct EventStoreRepository<A> {
    store: Arc<dyn EventStorePort>,
    tenant_id: String,
    aggregate_type: String,
    retry: RetryPolicy,
    page_size: u32,
    _phantom: PhantomData<fn() -> A>,
}

impl<A> EventStoreRepository<A>
where
    A: Aggregate,
{
    /// Create a repository for `tenant_id`. The aggregate type recorded on
    /// events defaults to [`Aggregate::aggregate_type`].
    pub fn new(store: Arc<dyn EventStorePort>, tenant_id: impl Into<String>) -> Self {
        Self {
            store,
            tenant_id: tenant_id.into(),
            aggregate_type: A::default().aggregate_type().to_string(),
            retry: RetryPolicy::default(),
            page_size: DEFAULT_PAGE_SIZE,
            _phantom: PhantomData,
        }
    }

    /// Override the aggregate type recorded on events.
    pub fn with_aggregate_type(mut self, aggregate_type: impl Into<String>) -> Self {
        self.aggregate_type = aggregate_type.into();
        self
    }

    /// Override the retry policy for unknown-outcome saves.
    pub fn with_retry_policy(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    /// Override the read page size used while replaying a stream.
    pub fn with_page_size(mut self, page_size: u32) -> Self {
        self.page_size = page_size.max(1);
        self
    }

    /// Aggregate type recorded on events.
    pub fn aggregate_type(&self) -> &str {
        &self.aggregate_type
    }

    /// Tenant scope of this repository.
    pub fn tenant_id(&self) -> &str {
        &self.tenant_id
    }

    async fn read_page(
        &self,
        aggregate_id: &str,
        from: u64,
        max_count: u32,
    ) -> Result<proto::ReadStreamResponse> {
        self.store
            .read_stream(proto::ReadStreamRequest {
                tenant_id: self.tenant_id.clone(),
                aggregate_id: aggregate_id.to_string(),
                from_aggregate_nonce: from,
                max_count,
                forward: true,
            })
            .await
    }

    fn check_type(&self, aggregate_id: &str, meta: &proto::EventMetadata) -> Result<()> {
        if meta.aggregate_type != self.aggregate_type {
            return Err(Error::Repository(anyhow::anyhow!(
                "stream '{aggregate_id}' belongs to aggregate type '{}', not '{}'",
                meta.aggregate_type,
                self.aggregate_type
            )));
        }
        Ok(())
    }

    /// Number of leading events of `batch` that the store already holds at
    /// the expected position (same event IDs, same nonces). A non-zero result
    /// means an earlier attempt committed (batches are atomic, so the prefix
    /// is one or more whole earlier batches).
    async fn committed_prefix_len(
        &self,
        aggregate_id: &str,
        expected: u64,
        batch: &[proto::EventData],
    ) -> Result<usize> {
        let page = self
            .read_page(aggregate_id, expected + 1, batch.len() as u32)
            .await?;
        Ok(page
            .events
            .iter()
            .zip(batch)
            .take_while(
                |(stored, ours)| match (stored.meta.as_ref(), ours.meta.as_ref()) {
                    (Some(s), Some(o)) => {
                        s.event_id == o.event_id
                            && s.aggregate_nonce == o.aggregate_nonce
                            && s.event_type == o.event_type
                            && stored.payload == ours.payload
                    }
                    _ => false,
                },
            )
            .count())
    }
}

impl<A> EventStoreRepository<A>
where
    A: Aggregate,
    A::Event: Serialize,
{
    fn to_event_data(
        &self,
        aggregate_id: &str,
        envelope: &EventEnvelope<A::Event>,
    ) -> Result<proto::EventData> {
        let m = &envelope.metadata;
        Ok(proto::EventData {
            meta: Some(proto::EventMetadata {
                event_id: m.event_id.to_string(),
                aggregate_id: aggregate_id.to_string(),
                aggregate_type: self.aggregate_type.clone(),
                aggregate_nonce: m.aggregate_nonce,
                event_type: envelope.event.event_type().to_string(),
                event_version: envelope.event.event_version(),
                content_type: CONTENT_TYPE_JSON.to_string(),
                content_schema: String::new(),
                correlation_id: m.correlation_id.clone().unwrap_or_default(),
                causation_id: m.causation_id.clone().unwrap_or_default(),
                actor_id: m.actor_id.clone().unwrap_or_default(),
                tenant_id: self.tenant_id.clone(),
                timestamp_unix_ms: m.timestamp.timestamp_millis().max(0) as u64,
                recorded_time_unix_ms: 0,
                payload_sha256: Vec::new(),
                headers: m.metadata.clone(),
                global_nonce: 0,
            }),
            payload: serde_json::to_vec(&envelope.event)?,
        })
    }

    /// Append request for the instance's (non-empty) pending batch.
    fn append_request(
        &self,
        aggregate_id: &str,
        instance: &AggregateInstance<A>,
    ) -> Result<proto::AppendRequest> {
        let pending = &instance.uncommitted_events;
        let expected = instance.committed_version();
        let events = pending
            .iter()
            .map(|e| self.to_event_data(aggregate_id, e))
            .collect::<Result<Vec<_>>>()?;
        let count = events.len();
        let first_id = pending[0].metadata.event_id;
        let last_id = pending[count - 1].metadata.event_id;
        Ok(proto::AppendRequest {
            tenant_id: self.tenant_id.clone(),
            aggregate_id: aggregate_id.to_string(),
            aggregate_type: self.aggregate_type.clone(),
            expected_aggregate_nonce: expected,
            // Same key on every attempt for this batch; a batch that changed
            // (more events recorded) gets a different key.
            idempotency_key: format!("esp-rs:{expected}:{count}:{first_id}:{last_id}"),
            events,
        })
    }
}

#[async_trait]
impl<A> Repository<A> for EventStoreRepository<A>
where
    A: Aggregate + 'static,
    A::Event: Serialize + DeserializeOwned + 'static,
{
    async fn load(&self, aggregate_id: &str) -> Result<Option<AggregateInstance<A>>> {
        let mut aggregate = A::default();
        let mut version = 0u64;
        let mut from = 1u64;
        loop {
            let page = self.read_page(aggregate_id, from, self.page_size).await?;
            let count = page.events.len();
            for data in page.events {
                let meta = data.meta.ok_or_else(|| {
                    Error::Repository(anyhow::anyhow!("stored event without metadata"))
                })?;
                self.check_type(aggregate_id, &meta)?;
                if meta.aggregate_nonce != version + 1 {
                    return Err(Error::Repository(anyhow::anyhow!(
                        "stream '{aggregate_id}' has a gap: expected nonce {}, got {}",
                        version + 1,
                        meta.aggregate_nonce
                    )));
                }
                let event: A::Event = serde_json::from_slice(&data.payload)?;
                aggregate.apply_event(&event)?;
                version = meta.aggregate_nonce;
            }
            if page.is_end || count == 0 {
                break;
            }
            from = version + 1;
        }

        if version == 0 {
            return Ok(None);
        }
        let mut instance =
            AggregateInstance::from_history(aggregate_id.to_string(), aggregate, version);
        instance.metadata.aggregate_type = self.aggregate_type.clone();
        Ok(Some(instance))
    }

    async fn save(&self, instance: &mut AggregateInstance<A>) -> Result<()> {
        if instance.uncommitted_events.is_empty() {
            return Ok(());
        }
        let aggregate_id = instance.aggregate_id().to_string();
        if aggregate_id.is_empty() {
            return Err(Error::invalid_state(
                "cannot save an aggregate without an id",
            ));
        }
        // Appending to an existing stream must not change its aggregate type.
        // Instance metadata is caller-controlled (fields are public,
        // `from_history` takes any type), so verify against the type stored
        // on the stream's first event. One single-event read per save of an
        // existing stream; a new stream (expected 0) needs no read because
        // the store rejects it if the stream already exists.
        if instance.committed_version() > 0 {
            let page = self.read_page(&aggregate_id, 1, 1).await?;
            if let Some(meta) = page.events.first().and_then(|e| e.meta.as_ref()) {
                self.check_type(&aggregate_id, meta)?;
            }
        }
        instance.metadata.aggregate_type = self.aggregate_type.clone();

        let max_attempts = self.retry.max_attempts.max(1);
        let mut backoff = self.retry.initial_backoff;
        let mut attempt = 1u32;
        // Each pass appends the current pending batch. A pass ends early only
        // when reconciliation finds that a prefix of the batch was committed
        // by an earlier attempt whose ack was lost (possibly before more
        // events were recorded on the instance). That prefix is cleared and
        // the remainder is appended on top of it. Pending strictly shrinks
        // between passes, so this terminates.
        'batch: loop {
            if instance.uncommitted_events.is_empty() {
                return Ok(());
            }
            let request = self.append_request(&aggregate_id, instance)?;
            let expected = request.expected_aggregate_nonce;
            let count = request.events.len() as u64;
            loop {
                match self.store.append(request.clone()).await {
                    Ok(resp) => {
                        if resp.last_aggregate_nonce != expected + count {
                            return Err(Error::Repository(anyhow::anyhow!(
                                "store acknowledged head {} for '{aggregate_id}', expected {}",
                                resp.last_aggregate_nonce,
                                expected + count
                            )));
                        }
                        instance.mark_committed();
                        return Ok(());
                    }
                    Err(err)
                        if err.is_concurrency_conflict()
                            || err.status_code() == Some(tonic::Code::AlreadyExists) =>
                    {
                        let committed = self
                            .committed_prefix_len(&aggregate_id, expected, &request.events)
                            .await?;
                        if committed == 0 {
                            return Err(err);
                        }
                        instance.uncommitted_events.drain(..committed);
                        continue 'batch;
                    }
                    Err(err) if err.is_transient() && attempt < max_attempts => {
                        tracing::warn!(
                            aggregate_id = %aggregate_id,
                            attempt,
                            error = %err,
                            "append outcome unknown; retrying idempotently"
                        );
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(self.retry.max_backoff);
                        attempt += 1;
                    }
                    Err(err) => return Err(err),
                }
            }
        }
    }

    async fn exists(&self, aggregate_id: &str) -> Result<bool> {
        let page = self.read_page(aggregate_id, 1, 1).await?;
        match page.events.first().and_then(|e| e.meta.as_ref()) {
            Some(meta) => {
                self.check_type(aggregate_id, meta)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }
}
