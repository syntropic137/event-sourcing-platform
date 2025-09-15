//! Command handling patterns and abstractions

use async_trait::async_trait;
use std::fmt::Debug;

use crate::error::Result;
use crate::event::DomainEvent;

/// Trait for command types
pub trait Command: Debug + Send + Sync {}

/// Trait for handling commands and producing events
#[async_trait]
pub trait CommandHandler<C, E>: Send + Sync
where
    C: Command,
    E: DomainEvent,
{
    /// Handle a command and return events to be persisted
    async fn handle(&self, command: C) -> Result<Vec<E>>;
}

/// A command handler that operates on an aggregate
#[async_trait]
pub trait AggregateCommandHandler<A, C, E>: Send + Sync
where
    C: Command,
    E: DomainEvent,
{
    /// Handle a command with access to the current aggregate state
    async fn handle(&self, aggregate: &A, command: C) -> Result<Vec<E>>;
}
