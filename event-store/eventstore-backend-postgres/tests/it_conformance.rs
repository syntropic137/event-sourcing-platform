//! Shared append idempotency/concurrency conformance suite (ADR-028)
//! against the Postgres backend (TEST_DATABASE_URL, else a testcontainer).
mod common;

use std::sync::Arc;

use eventstore_backend_postgres::PostgresStore;
use eventstore_core::EventStore;

async fn store() -> Arc<dyn EventStore> {
    let url = common::get_test_database_url().await;
    PostgresStore::connect_for_tests(&url)
        .await
        .expect("connect postgres")
}

eventstore_core::append_conformance_tests!(store);
