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

- Client hardening (#373, part of #302): `EventStoreClient::connect_with(ClientConfig)` with TLS (`TlsConfig`: custom CA, domain override, client identity; rustls/ring, OS roots by default), `https://` endpoints, connect and request timeouts, HTTP/2 and TCP keepalive, lazy connect, and `authorization` credentials (`basic_auth` for the ADR-024 gateway, `bearer_token`, `token_provider` / `SharedToken` for rotation). Credentials are refused over plaintext to non-loopback hosts unless `allow_insecure_credentials(true)`. `EventStoreClient::server_info`, `require_capabilities`, `require_min_version`; `Error::Incompatible`, `Error::Config`.
- Cross-language event envelope (ADR-027, #371): events are written and read exactly as the TypeScript and Python SDKs do (flat JSON body; `event_type`/`event_version` in metadata), verified by golden fixtures from the real TS/Python encoders and live round trips (`make test-xlang`).
- `EventSchema` (per-event struct with `EVENT_TYPE`/`EVENT_VERSION`) and the `event_enum!` macro, which generates dispatch-on-type encode/decode with compile-time checks for duplicate `(type, version)` pairs and invalid names (#371).
- `upcast::Upcasters` (`register`, `rename`, `upcast(event_type, version, Value)`), applied by `EventStoreRepository::with_upcasters` on load and `ProjectionRunner::with_upcasters` before routing (#371).
- `RecordedEvent::decode::<E: DomainEvent>()` (dispatching), `decode_with(&Upcasters)`, `payload_json::<T>()` (raw) (#371).
- Typed decode errors: `Error::UnknownEventType`, `UnknownEventVersion`, `EventDecode`, `UnsupportedContentType`, `Upcast`, `InvalidEvent` (#371).
- Supervised projection runner (#372): `ProjectionRunner::run_supervised(cancel, BackoffPolicy)` reconnects from the checkpoint on transient failures (`UNAVAILABLE`, `RESOURCE_EXHAUSTED`, transport, Postgres projection-store connection loss) with jittered exponential backoff and a cap (`BackoffPolicy`: initial, max, multiplier, jitter, `with_max_retries`); cancellation is prompt during backoff. Non-retryable errors stop with their typed error.
- `Error::DataLoss { global_nonce, message }` for gRPC `DATA_LOSS` carrying the `esp-undecodable-global-nonce` trailer (ADR-026). Runners never retry or skip it: catch-up applies every event before it, the checkpoint stays just below it, the runner halts (`RunnerHealth::halted_at`, one `ERROR` log per position, live processor held). Opt-in `with_undecodable_recheck(interval)` stays halted and resumes on its own after an operator fix, like the Python coordinator (#380). An undecodable head event is used as the live boundary.
- `ProjectionRunner::health()` (`RunnerHealth`: `state`, `position`, `live_boundary`, `lag()`, `halted_at`, `last_error`, `consecutive_failures`, `restarts`, `is_healthy()`).
- Capability guard: `run`, `catch_up` and supervised reconnects require `REQUIRED_CAPABILITIES` (`commit_ordered_global_nonce`, `subscription_errors_surfaced`, `undecodable_events_surfaced`); `with_required_capabilities`, `without_capability_check`. `EventStorePort::server_info` (default: legacy, so non-forwarding ports fail closed).
- `LiveProcessor` panics are caught: the runner stops with `Error::LiveProcessorPanicked` (default) or retries the pass with `on_processor_panic(ProcessorPanicPolicy::Restart)`. A panic racing with shutdown is still reported.
- `Error::CheckpointFenced { projection, stored, position }` for fenced checkpoint commits; `Error::data_loss_position()`.
- Example `supervised_projection` (projection service with supervision, live processor and health).

### Fixed

- `EventStoreClient::connect("https://...")` no longer becomes `http://https://...`; it now connects over TLS (#373).

### Breaking

- **Capability guard (#372).** `ProjectionRunner::run` and `catch_up` now refuse an event store that does not advertise `REQUIRED_CAPABILITIES` (`Error::Incompatible`). Custom `EventStorePort` implementations must forward `server_info` (the default reports a legacy server), or the runner must opt out with `without_capability_check()`.
- Fenced checkpoint commits are `Error::CheckpointFenced` instead of `Error::Repository` (#372).
- `DATA_LOSS` statuses with a position are `Error::DataLoss` instead of `Error::EventStore` (#372).
- A panicking `LiveProcessor` pass now stops the runner (`Error::LiveProcessorPanicked`) instead of silently ending the processor task (#372).

- Default request timeout of 30s on unary calls and on opening a subscription (open subscriptions have no deadline); pass `request_timeout(None)` to disable (#373).
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
