//! Aggregate abstractions and base implementations
//!
//! This module provides the core traits and types for implementing event-sourced
//! aggregates in Rust. Aggregates are the consistency boundaries in event sourcing,
//! handling commands and emitting events that represent state changes.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

use crate::error::{Error, Result};
use crate::event::DomainEvent;

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

/// A wrapper that combines an aggregate with its metadata
#[derive(Debug)]
pub struct AggregateInstance<A: Aggregate> {
    /// The aggregate root
    pub aggregate: A,
    /// Metadata about the aggregate
    pub metadata: AggregateMetadata,
    /// Uncommitted events
    pub uncommitted_events: Vec<A::Event>,
}

impl<A: Aggregate> AggregateInstance<A> {
    /// Create a new aggregate instance
    pub fn new(aggregate_id: String, aggregate: A) -> Self {
        let metadata = AggregateMetadata::new(aggregate_id, aggregate.aggregate_type().to_string());
        Self {
            aggregate,
            metadata,
            uncommitted_events: Vec::new(),
        }
    }

    /// Add uncommitted events
    pub fn add_events(&mut self, events: Vec<A::Event>) -> Result<()> {
        // Apply events to the aggregate
        self.aggregate.apply_events(&events)?;

        // Update metadata
        for _ in &events {
            self.metadata.increment_version();
        }

        // Track uncommitted events
        self.uncommitted_events.extend(events);

        Ok(())
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

        instance.mark_committed();
        assert_eq!(instance.uncommitted_count(), 0);
        assert!(!instance.has_uncommitted_events());
    }
}
