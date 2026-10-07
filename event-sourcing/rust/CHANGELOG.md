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

- Cross-language event envelope (ADR-026, #371): events are written and read exactly as the TypeScript and Python SDKs do (flat JSON body; `event_type`/`event_version` in metadata), verified by golden fixtures from the real TS/Python encoders and live round trips (`make test-xlang`).
- `EventSchema` (per-event struct with `EVENT_TYPE`/`EVENT_VERSION`) and the `event_enum!` macro, which generates dispatch-on-type encode/decode with compile-time checks for duplicate `(type, version)` pairs and invalid names (#371).
- `upcast::Upcasters` (`register`, `rename`, `upcast(event_type, version, Value)`), applied by `EventStoreRepository::with_upcasters` on load and `ProjectionRunner::with_upcasters` before routing (#371).
- `RecordedEvent::decode::<E: DomainEvent>()` (dispatching), `decode_with(&Upcasters)`, `payload_json::<T>()` (raw) (#371).
- Typed decode errors: `Error::UnknownEventType`, `UnknownEventVersion`, `EventDecode`, `UnsupportedContentType`, `Upcast`, `InvalidEvent` (#371).

### Breaking

- **Wire format (#371).** The payload is the event's flat JSON body instead of the serde enum (`{"amount":5}`, not `{"Deposited":{"amount":5}}`). Streams written by earlier pre-release builds of this crate are not readable; there was no release with the old format.
- **`DomainEvent` (#371)** requires `to_payload` and `from_payload` and is `Sized`. Implement it through `EventSchema` / `event_enum!` (see the `event` module docs); unit and tuple payloads are rejected (`Error::InvalidEvent`), use `struct E {}`.
- **`Aggregate::AGGREGATE_TYPE` (#371)** is a required associated const (stream identity shared with TS/Python, validated at compile time). `Aggregate::aggregate_type()` (which defaulted to `std::any::type_name`) and `EventStoreRepository::with_aggregate_type` are removed.
- **`RecordedEvent::decode` (#371)** now takes `E: DomainEvent` and checks type and version; the old raw behavior is `payload_json`. `RecordedEvent::event_version` reports wire `0` as `1`.
- Repository bounds no longer require `A::Event: Serialize + DeserializeOwned`.

- Removed the placeholder `projection::ProjectionManager` and the uncheckpointed `projection::Projection<E>` trait; use `CheckpointedProjection` with `ProjectionRunner` (#353).

- `EventStoreClient::new(String)` (which ignored its address) is replaced by `async EventStoreClient::connect(addr) -> Result<Self>`.
- `Repository` now works on `AggregateInstance<A>`: `load -> Option<AggregateInstance<A>>`, `save(&mut AggregateInstance<A>)`.
- `AggregateInstance::uncommitted_events` is now `Vec<EventEnvelope<A::Event>>` (stable id, timestamp, nonce per pending event).
- `AggregateInstance::add_events` and `execute` require `A: Clone` so a batch that fails to apply leaves the instance unchanged.
