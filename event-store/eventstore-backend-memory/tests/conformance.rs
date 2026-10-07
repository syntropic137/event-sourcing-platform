//! Shared append idempotency/concurrency conformance suite (ADR-028)
//! against the memory backend.

use std::sync::Arc;

use eventstore_backend_memory::InMemoryStore;
use eventstore_core::EventStore;

async fn store() -> Arc<dyn EventStore> {
    InMemoryStore::new()
}

eventstore_core::append_conformance_tests!(store);
