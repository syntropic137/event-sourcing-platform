//! Aggregate abstractions and base implementations
//!
//! This module provides the core traits and types for implementing event-sourced
//! aggregates in Rust. Aggregates are the consistency boundaries in event sourcing,
//! handling commands and emitting events that represent state changes.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

use crate::error::{Error, Result};
use crate::event::{DomainEvent, EventEnvelope};

/// Core trait for event-sourced aggregates
///
/// An aggregate represents a consistency boundary that processes commands
/// and emits events. The aggregate's state is derived by replaying events
/// in order.
pub trait Aggregate: Debug + Default + Send + Sync {
    /// The type of events this aggregate can apply
    type Event: DomainEvent;

    /// Error type for this aggregate
    type Error: Into<Error>;

    /// Get the aggregate's identifier
    fn aggregate_id(&self) -> Option<&str>;

    /// Get the aggregate's type name
    fn aggregate_type(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    /// Get the current version of the aggregate
    fn version(&self) -> u64;

    /// Apply an event to the aggregate, evolving its state
    ///
    /// This method should be pure and deterministic - given the same
    /// sequence of events, it should always produce the same state.
    fn apply_event(&mut self, event: &Self::Event) -> Result<()>;

    /// Apply multiple events in sequence
    fn apply_events(&mut self, events: &[Self::Event]) -> Result<()> {
        for event in events {
            self.apply_event(event)?;
        }
        Ok(())
    }

    /// Check if the aggregate exists (has been initialized)
    fn exists(&self) -> bool {
        self.aggregate_id().is_some() && self.version() > 0
    }
}

/// Extended aggregate trait for aggregates that can be loaded from events
#[async_trait]
pub trait AggregateLoader<A: Aggregate>: Send + Sync
where
    A::Event: Send + Sync + 'static,
{
    /// Load an aggregate from a sequence of events
    async fn load_from_events(&self, events: Vec<A::Event>) -> Result<A> {
        let mut aggregate = A::default();
        aggregate.apply_events(&events)?;
        Ok(aggregate)
    }
}

/// A root aggregate that can handle commands and emit events
#[async_trait]
pub trait AggregateRoot: Aggregate {
    /// Command type this aggregate can handle
    type Command: Send + Sync;

    /// Handle a command and return events to be persisted
    ///
    /// This method should contain the business logic for validating
    /// the command against the current state and deciding what events
    /// to emit.
    async fn handle_command(&self, command: Self::Command) -> Result<Vec<Self::Event>>;
}

/// Metadata about an aggregate instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateMetadata {
    /// The aggregate's unique identifier
    pub aggregate_id: String,
    /// The aggregate's type
    pub aggregate_type: String,
    /// Current version/sequence number
    pub version: u64,
    /// When the aggregate was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the aggregate was last updated
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl AggregateMetadata {
    /// Create new metadata for an aggregate
    pub fn new(aggregate_id: String, aggregate_type: String) -> Self {
        let now = chrono::Utc::now();
        Self {
            aggregate_id,
            aggregate_type,
            version: 0,
            created_at: now,
            updated_at: now,
        }
    }

    /// Update the version and timestamp
    pub fn increment_version(&mut self) {
        self.version += 1;
        self.updated_at = chrono::Utc::now();
    }
}

/// A unit of work: an aggregate, its stream revision, and its pending events.
///
/// `metadata.version` is the stream revision *including* pending events.
/// [`committed_version`](Self::committed_version) is the revision the store
/// last acknowledged and is sent as the expected revision on save.
///
/// Pending events are recorded as [`EventEnvelope`]s with their event ID,
/// timestamp, and aggregate nonce fixed at record time. A retried save sends
/// byte-identical events, which lets the store (and the repository) recognize
/// a batch that was already committed before an acknowledgment was lost.
#[derive(Debug)]
pub struct AggregateInstance<A: Aggregate> {
    /// The aggregate root
    pub aggregate: A,
    /// Metadata about the aggregate
    pub metadata: AggregateMetadata,
    /// Uncommitted (pending) events, in stream order
    pub uncommitted_events: Vec<EventEnvelope<A::Event>>,
}

impl<A: Aggregate> AggregateInstance<A> {
    /// Create a new aggregate instance for a stream that does not exist yet.
    pub fn new(aggregate_id: String, aggregate: A) -> Self {
        let metadata = AggregateMetadata::new(aggregate_id, aggregate.aggregate_type().to_string());
        Self {
            aggregate,
            metadata,
            uncommitted_events: Vec::new(),
        }
    }

    /// Create an instance rehydrated from `version` committed events.
    pub fn from_history(aggregate_id: String, aggregate: A, version: u64) -> Self {
        let mut instance = Self::new(aggregate_id, aggregate);
        instance.metadata.version = version;
        instance
    }

    /// The aggregate's identifier (stream id).
    pub fn aggregate_id(&self) -> &str {
        &self.metadata.aggregate_id
    }

    /// Stream revision acknowledged by the store (excludes pending events).
    pub fn committed_version(&self) -> u64 {
        self.metadata.version - self.uncommitted_events.len() as u64
    }

    /// Apply new events to the aggregate and record them as pending.
    pub fn add_events(&mut self, events: Vec<A::Event>) -> Result<()> {
        // Apply events to the aggregate
        self.aggregate.apply_events(&events)?;

        for event in events {
            self.metadata.increment_version();
            self.uncommitted_events.push(EventEnvelope::new(
                event,
                self.metadata.aggregate_id.clone(),
                self.metadata.aggregate_type.clone(),
                self.metadata.version,
            ));
        }

        Ok(())
    }

    /// Run a command against the current state and record the resulting events.
    ///
    /// Returns the number of events recorded.
    pub async fn execute(&mut self, command: A::Command) -> Result<usize>
    where
        A: AggregateRoot,
    {
        let events = self.aggregate.handle_command(command).await?;
        let count = events.len();
        self.add_events(events)?;
        Ok(count)
    }

    /// Pending domain events, in stream order.
    pub fn pending_events(&self) -> impl Iterator<Item = &A::Event> {
        self.uncommitted_events.iter().map(|e| &e.event)
    }

    /// Mark all events as committed
    pub fn mark_committed(&mut self) {
        self.uncommitted_events.clear();
    }

    /// Get the number of uncommitted events
    pub fn uncommitted_count(&self) -> usize {
        self.uncommitted_events.len()
    }

    /// Check if there are uncommitted events
    pub fn has_uncommitted_events(&self) -> bool {
        !self.uncommitted_events.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    enum TestEvent {
        Created { id: String },
        Updated { value: i32 },
    }

    impl DomainEvent for TestEvent {
        fn event_type(&self) -> &'static str {
            match self {
                TestEvent::Created { .. } => "TestCreated",
                TestEvent::Updated { .. } => "TestUpdated",
            }
        }

        fn event_version(&self) -> u32 {
            1
        }
    }

    #[derive(Debug, Default)]
    struct TestAggregate {
        id: Option<String>,
        value: i32,
        version: u64,
    }

    impl Aggregate for TestAggregate {
        type Event = TestEvent;
        type Error = Error;

        fn aggregate_id(&self) -> Option<&str> {
            self.id.as_deref()
        }

        fn version(&self) -> u64 {
            self.version
        }

        fn apply_event(&mut self, event: &Self::Event) -> Result<()> {
            match event {
                TestEvent::Created { id } => {
                    self.id = Some(id.clone());
                }
                TestEvent::Updated { value } => {
                    self.value = *value;
                }
            }
            self.version += 1;
            Ok(())
        }
    }

    #[test]
    fn test_aggregate_apply_events() {
        let mut aggregate = TestAggregate::default();

        let events = vec![
            TestEvent::Created {
                id: "test-1".to_string(),
            },
            TestEvent::Updated { value: 42 },
        ];

        aggregate.apply_events(&events).unwrap();

        assert_eq!(aggregate.aggregate_id(), Some("test-1"));
        assert_eq!(aggregate.value, 42);
        assert_eq!(aggregate.version(), 2);
        assert!(aggregate.exists());
    }

    #[test]
    fn test_aggregate_instance() {
        let aggregate = TestAggregate::default();
        let mut instance = AggregateInstance::new("test-1".to_string(), aggregate);

        let events = vec![
            TestEvent::Created {
                id: "test-1".to_string(),
            },
            TestEvent::Updated { value: 100 },
        ];

        instance.add_events(events).unwrap();

        assert_eq!(instance.uncommitted_count(), 2);
        assert!(instance.has_uncommitted_events());
        assert_eq!(instance.metadata.version, 2);
        assert_eq!(instance.committed_version(), 0);
        let nonces: Vec<u64> = instance
            .uncommitted_events
            .iter()
            .map(|e| e.aggregate_nonce())
            .collect();
        assert_eq!(nonces, vec![1, 2]);

        instance.mark_committed();
        assert_eq!(instance.uncommitted_count(), 0);
        assert!(!instance.has_uncommitted_events());
        assert_eq!(instance.committed_version(), 2);

        instance
            .add_events(vec![TestEvent::Updated { value: 7 }])
            .unwrap();
        assert_eq!(instance.committed_version(), 2);
        assert_eq!(instance.uncommitted_events[0].aggregate_nonce(), 3);
    }
}
