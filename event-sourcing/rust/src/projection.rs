//! Projection building and read model management

use async_trait::async_trait;

use crate::error::Result;
use crate::event::DomainEvent;

/// Trait for event projections that build read models
#[async_trait]
pub trait Projection<E>: Send + Sync
where
    E: DomainEvent,
{
    /// Handle an event and update the projection
    async fn handle_event(&mut self, event: &E) -> Result<()>;

    /// Get the projection name
    fn name(&self) -> &str;
}

/// A projection manager that coordinates multiple projections
pub struct ProjectionManager {
    // TODO: Add projection tracking and management
}

impl ProjectionManager {
    /// Create a new projection manager
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for ProjectionManager {
    fn default() -> Self {
        Self::new()
    }
}
