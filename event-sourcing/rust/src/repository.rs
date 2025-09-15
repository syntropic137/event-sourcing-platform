//! Repository pattern for loading and saving aggregates

use async_trait::async_trait;

use crate::aggregate::Aggregate;
use crate::error::Result;

/// Repository trait for loading and saving aggregates
#[async_trait]
pub trait Repository<A>: Send + Sync
where
    A: Aggregate,
{
    /// Load an aggregate by ID
    async fn load(&self, aggregate_id: &str) -> Result<Option<A>>;

    /// Save an aggregate
    async fn save(&self, aggregate: &A) -> Result<()>;

    /// Check if an aggregate exists
    async fn exists(&self, aggregate_id: &str) -> Result<bool>;
}

/// Alias for the repository trait with clearer naming
pub type AggregateRepository<A> = dyn Repository<A>;

/// Repository implementation that uses the event store
pub struct EventStoreRepository<A> {
    _phantom: std::marker::PhantomData<A>,
}

impl<A> Default for EventStoreRepository<A>
where
    A: Aggregate,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<A> EventStoreRepository<A>
where
    A: Aggregate,
{
    /// Create a new event store repository
    pub fn new() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<A> Repository<A> for EventStoreRepository<A>
where
    A: Aggregate,
{
    async fn load(&self, _aggregate_id: &str) -> Result<Option<A>> {
        // TODO: Implement loading from event store
        todo!("Implement loading from event store")
    }

    async fn save(&self, _aggregate: &A) -> Result<()> {
        // TODO: Implement saving to event store
        todo!("Implement saving to event store")
    }

    async fn exists(&self, _aggregate_id: &str) -> Result<bool> {
        // TODO: Implement existence check
        todo!("Implement existence check")
    }
}
