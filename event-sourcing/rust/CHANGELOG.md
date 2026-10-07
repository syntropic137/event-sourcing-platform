# Changelog

All notable changes to the Rust Event Sourcing SDK (`event-sourcing-rust`) will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
The SDK is alpha: breaking changes may land in minor versions and are listed under **Breaking**.

## [Unreleased]

### Added

- `client::EventStorePort` trait and `EventStoreClient::connect`, layered on the low-level `eventstore-sdk-rs` client (#352).
- `EventStoreRepository`: `load` (paged replay), `save` (expected stream revision, typed `Error::ConcurrencyConflict`), `exists` (#352).
- Idempotent retry of unknown-outcome saves (`RetryPolicy`, stable event ids and idempotency key, reconciliation by event id and payload) (#352).
- `AggregateInstance::execute`, `committed_version`, `from_history`, `pending_events`; `Error::is_transient`, `is_concurrency_conflict`, `status_code` (#352).

- Checkpointed `ProjectionRunner` (#353): catch-up then live, persisted resume per `CheckpointKey` (tenant, projection name, version, feed), atomic read model + checkpoint commits via `ProjectionStore` (`InMemoryProjectionStore`, `PostgresProjectionStore` behind the `postgres` feature), `ExternalCheckpoints` for idempotent external read models, per-key `rebuild`, `LiveProcessor` for live-only side effects, `Error::ProjectionFailed` and `Error::OutOfOrderDelivery`.

### Breaking

- Removed the placeholder `projection::ProjectionManager` and the uncheckpointed `projection::Projection<E>` trait; use `CheckpointedProjection` with `ProjectionRunner` (#353).

- `EventStoreClient::new(String)` (which ignored its address) is replaced by `async EventStoreClient::connect(addr) -> Result<Self>`.
- `Repository` now works on `AggregateInstance<A>`: `load -> Option<AggregateInstance<A>>`, `save(&mut AggregateInstance<A>)`.
- `AggregateInstance::uncommitted_events` is now `Vec<EventEnvelope<A::Event>>` (stable id, timestamp, nonce per pending event).
- `AggregateInstance::add_events` and `execute` require `A: Clone` so a batch that fails to apply leaves the instance unchanged.
