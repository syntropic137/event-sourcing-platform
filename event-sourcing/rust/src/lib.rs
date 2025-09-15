//! # Event Sourcing Rust SDK
//!
//! This crate provides high-level abstractions for implementing event sourcing patterns
//! in Rust applications. It builds on top of the event-store gRPC API to provide
//! developer-friendly APIs for aggregates, commands, events, and repositories.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use event_sourcing_rust::prelude::*;
//! use serde::{Deserialize, Serialize};
//! use uuid::Uuid;
//!
//! // Define your events
//! #[derive(Debug, Clone, Serialize, Deserialize)]
//! pub enum OrderEvent {
//!     OrderSubmitted { order_id: String, customer_id: String },
//!     OrderCancelled { reason: String },
//! }
//!
//! // Define your aggregate
//! #[derive(Debug, Default)]
//! pub struct OrderAggregate {
//!     id: Option<String>,
//!     status: OrderStatus,
//!     version: u64,
//! }
//!
//! #[derive(Debug, Default, PartialEq)]
//! pub enum OrderStatus {
//!     #[default]
//!     New,
//!     Submitted,
//!     Cancelled,
//! }
//!
//! impl DomainEvent for OrderEvent {
//!     fn event_type(&self) -> &'static str {
//!         match self {
//!             OrderEvent::OrderSubmitted { .. } => "OrderSubmitted",
//!             OrderEvent::OrderCancelled { .. } => "OrderCancelled",
//!         }
//!     }
//! }
//!
//! impl Aggregate for OrderAggregate {
//!     type Event = OrderEvent;
//!     type Error = Error;
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
//!             OrderEvent::OrderSubmitted { order_id, .. } => {
//!                 self.id = Some(order_id.clone());
//!                 self.status = OrderStatus::Submitted;
//!                 self.version += 1;
//!             }
//!             OrderEvent::OrderCancelled { .. } => {
//!                 self.status = OrderStatus::Cancelled;
//!                 self.version += 1;
//!             }
//!         }
//!         Ok(())
//!     }
//! }
//!
//! // Define commands
//! #[derive(Debug)]
//! pub struct SubmitOrder {
//!     pub order_id: String,
//!     pub customer_id: String,
//! }
//!
//! impl Command for SubmitOrder {}
//!
//! impl OrderAggregate {
//!     pub fn submit(&self, cmd: SubmitOrder) -> Result<Vec<OrderEvent>> {
//!         if self.status != OrderStatus::New {
//!             return Err(Error::invalid_state("Order already submitted"));
//!         }
//!
//!         Ok(vec![OrderEvent::OrderSubmitted {
//!             order_id: cmd.order_id,
//!             customer_id: cmd.customer_id,
//!         }])
//!     }
//! }
//! ```
//!
//! ## Architecture
//!
//! The SDK is organized into several key modules:
//!
//! - [`aggregate`] - Core aggregate traits and base implementations
//! - [`command`] - Command handling patterns and abstractions
//! - [`event`] - Event definitions and metadata handling
//! - [`repository`] - Repository pattern for loading/saving aggregates
//! - [`projection`] - Projection building and read model management
//! - [`client`] - Low-level event store client integration

pub mod aggregate;
pub mod client;
pub mod command;
pub mod error;
pub mod event;
pub mod projection;
pub mod repository;

/// Re-exports of commonly used types and traits
pub mod prelude {
    pub use crate::aggregate::{Aggregate, AggregateLoader, AggregateRoot};
    pub use crate::command::{Command, CommandHandler};
    pub use crate::error::{Error, Result};
    pub use crate::event::{DomainEvent, EventEnvelope, EventMetadata};
    pub use crate::repository::{AggregateRepository, Repository};

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
