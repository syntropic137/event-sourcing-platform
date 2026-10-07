//! # Event Sourcing Rust SDK
//!
//! This crate provides high-level abstractions for implementing event sourcing patterns
//! in Rust applications. It builds on top of the event-store gRPC API to provide
//! developer-friendly APIs for aggregates, commands, events, and repositories.
//!
//! Events use the cross-language envelope of ADR-026 ([`wire`]): streams
//! written by this SDK are readable by the TypeScript and Python SDKs, and
//! vice versa.
//!
//! ## Quick Start
//!
//! ```rust
//! use event_sourcing_rust::prelude::*;
//!
//! // One struct per event; the struct's fields are the JSON payload.
//! #[derive(Debug, Clone, Serialize, Deserialize)]
//! pub struct OrderSubmitted {
//!     pub order_id: String,
//!     pub customer_id: String,
//! }
//! impl EventSchema for OrderSubmitted {
//!     const EVENT_TYPE: &'static str = "OrderSubmitted";
//! }
//!
//! #[derive(Debug, Clone, Serialize, Deserialize)]
//! pub struct OrderCancelled {
//!     pub reason: String,
//! }
//! impl EventSchema for OrderCancelled {
//!     const EVENT_TYPE: &'static str = "OrderCancelled";
//! }
//!
//! // The aggregate's events: encode and decode dispatch on the event type.
//! event_sourcing_rust::event_enum! {
//!     #[derive(Debug, Clone)]
//!     pub enum OrderEvent {
//!         Submitted(OrderSubmitted),
//!         Cancelled(OrderCancelled),
//!     }
//! }
//!
//! #[derive(Debug, Default, Clone)]
//! pub struct Order {
//!     id: Option<String>,
//!     status: OrderStatus,
//!     version: u64,
//! }
//!
//! #[derive(Debug, Default, Clone, PartialEq)]
//! pub enum OrderStatus {
//!     #[default]
//!     New,
//!     Submitted,
//!     Cancelled,
//! }
//!
//! impl Aggregate for Order {
//!     type Event = OrderEvent;
//!     type Error = Error;
//!     // Stable stream identity, shared with the TypeScript/Python SDKs.
//!     const AGGREGATE_TYPE: &'static str = "Order";
//!
//!     fn aggregate_id(&self) -> Option<&str> {
//!         self.id.as_deref()
//!     }
//!
//!     fn version(&self) -> u64 {
//!         self.version
//!     }
//!
//!     fn apply_event(&mut self, event: &Self::Event) -> Result<()> {
//!         match event {
//!             OrderEvent::Submitted(e) => {
//!                 self.id = Some(e.order_id.clone());
//!                 self.status = OrderStatus::Submitted;
//!             }
//!             OrderEvent::Cancelled(_) => self.status = OrderStatus::Cancelled,
//!         }
//!         self.version += 1;
//!         Ok(())
//!     }
//! }
//!
//! impl Order {
//!     pub fn submit(&self, order_id: String, customer_id: String) -> Result<Vec<OrderEvent>> {
//!         if self.status != OrderStatus::New {
//!             return Err(Error::invalid_state("Order already submitted"));
//!         }
//!         Ok(vec![OrderSubmitted { order_id, customer_id }.into()])
//!     }
//! }
//!
//! let mut order = AggregateInstance::new("order-1".into(), Order::default());
//! let events = order.aggregate.submit("order-1".into(), "c-1".into()).unwrap();
//! order.add_events(events).unwrap();
//! assert_eq!(order.uncommitted_count(), 1);
//! ```
//!
//! ## Architecture
//!
//! The SDK is organized into several key modules:
//!
//! - [`aggregate`] - Core aggregate traits and base implementations
//! - [`command`] - Command handling patterns and abstractions
//! - [`event`] - Event definitions and metadata handling
//! - [`repository`] - Event store repository: load/replay, save with optimistic
//!   concurrency, idempotent retry of unknown-outcome saves
//! - [`projection`] - Checkpointed projection runner (catch-up, live, resume,
//!   rebuild) with transactional and external checkpoint stores
//! - [`wire`] - The cross-language event envelope (ADR-026) shared with the
//!   TypeScript and Python SDKs
//! - [`upcast`] - Upcasters that migrate stored events to the current schema
//! - [`client`] - gRPC event store client (layered on `eventstore-sdk-rs`) and
//!   the [`client::EventStorePort`] trait used for testing and decoration

pub mod aggregate;
pub mod client;
pub mod command;
pub mod error;
pub mod event;
pub mod projection;
pub mod repository;
pub mod upcast;
pub mod wire;

/// Re-exports of commonly used types and traits
pub mod prelude {
    pub use crate::aggregate::{Aggregate, AggregateInstance, AggregateLoader, AggregateRoot};
    pub use crate::client::{EventStoreClient, EventStorePort};
    pub use crate::command::{Command, CommandHandler};
    pub use crate::error::{Error, Result};
    pub use crate::event::{
        DomainEvent, EventEnvelope, EventMetadata, EventSchema, SerializedEvent,
    };
    pub use crate::projection::{
        CheckpointKey, CheckpointStore, CheckpointedProjection, DispatchContext,
        ExternalCheckpoints, InMemoryCheckpointStore, InMemoryProjectionStore, LiveProcessor,
        ProjectionRunner, ProjectionStore, RecordedEvent, RunExit,
    };
    pub use crate::repository::{
        AggregateRepository, EventStoreRepository, Repository, RetryPolicy,
    };
    pub use crate::upcast::Upcasters;

    // Re-export common external types
    pub use async_trait::async_trait;
    pub use chrono::{DateTime, Utc};
    pub use serde::{Deserialize, Serialize};
    pub use uuid::Uuid;
}

#[cfg(test)]
mod tests {
    use super::prelude::*;

    #[test]
    fn test_basic_imports() {
        // Just ensure all the basic imports work
        let _uuid = Uuid::new_v4();
        let _now = chrono::Utc::now();
    }
}
