//! Event definitions and metadata handling
//!
//! This module provides traits and types for working with domain events,
//! event envelopes, and event metadata in the event sourcing system.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Debug;
use uuid::Uuid;

/// Trait for domain events
///
/// Domain events represent facts that have occurred in the system.
/// They should be immutable and contain all the information needed
/// to understand what happened.
pub trait DomainEvent: Debug + Clone + Send + Sync {
    /// Get the event type identifier
    ///
    /// This should be a stable identifier that can be used for
    /// deserialization and event handling routing.
    fn event_type(&self) -> &'static str;

    /// Get the schema version of this event
    ///
    /// This is used for event upcasting and migration when
    /// event schemas evolve over time.
    fn event_version(&self) -> u32 {
        1
    }

    /// Get optional correlation ID for tracing related events
    fn correlation_id(&self) -> Option<&str> {
        None
    }

    /// Get optional causation ID for event causality tracking  
    fn causation_id(&self) -> Option<&str> {
        None
    }
}

/// Event metadata containing system-level information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMetadata {
    /// Unique identifier for this event
    pub event_id: Uuid,
    /// Type of the event
    pub event_type: String,
    /// Schema version of the event
    pub event_version: u32,
    /// Content type of the payload (e.g., "application/json")
    pub content_type: String,
    /// When the event occurred
    pub timestamp: DateTime<Utc>,
    /// ID of the aggregate that emitted this event
    pub aggregate_id: String,
    /// Type of the aggregate that emitted this event
    pub aggregate_type: String,
    /// Sequence number within the aggregate
    pub aggregate_nonce: u64,
    /// Global sequence number across all events
    pub global_nonce: Option<u64>,
    /// Optional correlation ID for request tracing
    pub correlation_id: Option<String>,
    /// Optional causation ID for event causality
    pub causation_id: Option<String>,
    /// Optional actor/user ID who caused this event
    pub actor_id: Option<String>,
    /// Optional tenant ID for multi-tenant systems
    pub tenant_id: Option<String>,
    /// Additional custom metadata
    pub metadata: HashMap<String, String>,
}

impl EventMetadata {
    /// Create new event metadata
    pub fn new(
        event_type: String,
        event_version: u32,
        aggregate_id: String,
        aggregate_type: String,
        aggregate_nonce: u64,
    ) -> Self {
        Self {
            event_id: Uuid::new_v4(), // TODO: Use v7 with timestamp when available
            event_type,
            event_version,
            content_type: "application/json".to_string(),
            timestamp: Utc::now(),
            aggregate_id,
            aggregate_type,
            aggregate_nonce,
            global_nonce: None,
            correlation_id: None,
            causation_id: None,
            actor_id: None,
            tenant_id: None,
            metadata: HashMap::new(),
        }
    }

    /// Set the correlation ID
    pub fn with_correlation_id(mut self, correlation_id: String) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    /// Set the causation ID
    pub fn with_causation_id(mut self, causation_id: String) -> Self {
        self.causation_id = Some(causation_id);
        self
    }

    /// Set the actor ID
    pub fn with_actor_id(mut self, actor_id: String) -> Self {
        self.actor_id = Some(actor_id);
        self
    }

    /// Set the tenant ID
    pub fn with_tenant_id(mut self, tenant_id: String) -> Self {
        self.tenant_id = Some(tenant_id);
        self
    }

    /// Add custom metadata
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Set the global nonce (typically set by the event store)
    pub fn with_global_nonce(mut self, global_nonce: u64) -> Self {
        self.global_nonce = Some(global_nonce);
        self
    }
}

/// An event envelope containing both the event data and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope<E> {
    /// Event metadata
    pub metadata: EventMetadata,
    /// The actual event data
    pub event: E,
}

impl<E> EventEnvelope<E>
where
    E: DomainEvent,
{
    /// Create a new event envelope
    pub fn new(
        event: E,
        aggregate_id: String,
        aggregate_type: String,
        aggregate_nonce: u64,
    ) -> Self {
        let mut metadata = EventMetadata::new(
            event.event_type().to_string(),
            event.event_version(),
            aggregate_id,
            aggregate_type,
            aggregate_nonce,
        );

        // Use correlation/causation from event if available
        if let Some(correlation_id) = event.correlation_id() {
            metadata = metadata.with_correlation_id(correlation_id.to_string());
        }
        if let Some(causation_id) = event.causation_id() {
            metadata = metadata.with_causation_id(causation_id.to_string());
        }

        Self { metadata, event }
    }

    /// Get the event ID
    pub fn event_id(&self) -> Uuid {
        self.metadata.event_id
    }

    /// Get the aggregate ID
    pub fn aggregate_id(&self) -> &str {
        &self.metadata.aggregate_id
    }

    /// Get the aggregate type
    pub fn aggregate_type(&self) -> &str {
        &self.metadata.aggregate_type
    }

    /// Get the aggregate nonce
    pub fn aggregate_nonce(&self) -> u64 {
        self.metadata.aggregate_nonce
    }

    /// Get the global nonce if available
    pub fn global_nonce(&self) -> Option<u64> {
        self.metadata.global_nonce
    }

    /// Get the timestamp
    pub fn timestamp(&self) -> DateTime<Utc> {
        self.metadata.timestamp
    }
}

/// Builder for creating event context with tracing information
#[derive(Debug, Default)]
pub struct EventContext {
    correlation_id: Option<String>,
    causation_id: Option<String>,
    actor_id: Option<String>,
    tenant_id: Option<String>,
    metadata: HashMap<String, String>,
}

impl EventContext {
    /// Create a new event context
    pub fn new() -> Self {
        Self::default()
    }

    /// Set correlation ID
    pub fn with_correlation_id(mut self, correlation_id: String) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    /// Set causation ID
    pub fn with_causation_id(mut self, causation_id: String) -> Self {
        self.causation_id = Some(causation_id);
        self
    }

    /// Set actor ID
    pub fn with_actor_id(mut self, actor_id: String) -> Self {
        self.actor_id = Some(actor_id);
        self
    }

    /// Set tenant ID
    pub fn with_tenant_id(mut self, tenant_id: String) -> Self {
        self.tenant_id = Some(tenant_id);
        self
    }

    /// Add custom metadata
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Apply this context to event metadata
    pub fn apply_to_metadata(&self, metadata: &mut EventMetadata) {
        if let Some(ref correlation_id) = self.correlation_id {
            metadata.correlation_id = Some(correlation_id.clone());
        }
        if let Some(ref causation_id) = self.causation_id {
            metadata.causation_id = Some(causation_id.clone());
        }
        if let Some(ref actor_id) = self.actor_id {
            metadata.actor_id = Some(actor_id.clone());
        }
        if let Some(ref tenant_id) = self.tenant_id {
            metadata.tenant_id = Some(tenant_id.clone());
        }
        for (key, value) in &self.metadata {
            metadata.metadata.insert(key.clone(), value.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct TestEvent {
        message: String,
    }

    impl DomainEvent for TestEvent {
        fn event_type(&self) -> &'static str {
            "TestEvent"
        }

        fn event_version(&self) -> u32 {
            1
        }
    }

    #[test]
    fn test_event_metadata_creation() {
        let metadata = EventMetadata::new(
            "TestEvent".to_string(),
            1,
            "test-123".to_string(),
            "TestAggregate".to_string(),
            5,
        );

        assert_eq!(metadata.event_type, "TestEvent");
        assert_eq!(metadata.event_version, 1);
        assert_eq!(metadata.aggregate_id, "test-123");
        assert_eq!(metadata.aggregate_type, "TestAggregate");
        assert_eq!(metadata.aggregate_nonce, 5);
        assert!(metadata.global_nonce.is_none());
    }

    #[test]
    fn test_event_envelope() {
        let event = TestEvent {
            message: "Hello, World!".to_string(),
        };

        let envelope = EventEnvelope::new(
            event,
            "test-123".to_string(),
            "TestAggregate".to_string(),
            1,
        );

        assert_eq!(envelope.aggregate_id(), "test-123");
        assert_eq!(envelope.aggregate_type(), "TestAggregate");
        assert_eq!(envelope.aggregate_nonce(), 1);
        assert_eq!(envelope.metadata.event_type, "TestEvent");
        assert_eq!(envelope.event.message, "Hello, World!");
    }

    #[test]
    fn test_event_context() {
        let context = EventContext::new()
            .with_correlation_id("corr-123".to_string())
            .with_actor_id("user-456".to_string())
            .with_metadata("custom".to_string(), "value".to_string());

        let mut metadata = EventMetadata::new(
            "TestEvent".to_string(),
            1,
            "test-123".to_string(),
            "TestAggregate".to_string(),
            1,
        );

        context.apply_to_metadata(&mut metadata);

        assert_eq!(metadata.correlation_id, Some("corr-123".to_string()));
        assert_eq!(metadata.actor_id, Some("user-456".to_string()));
        assert_eq!(metadata.metadata.get("custom"), Some(&"value".to_string()));
    }
}
