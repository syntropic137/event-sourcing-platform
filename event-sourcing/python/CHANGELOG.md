# Changelog

All notable changes to the Python Event Sourcing SDK will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed (long streams load whole, #405)

- `GrpcEventStoreClient.read_events` read one 1000-event ReadStream page and
  stopped, so a stream over 1000 events loaded truncated: its aggregate
  rehydrated with the wrong state and a stale version, and its next save
  failed OCC. It now follows `next_from_aggregate_nonce` until `is_end` (or an
  empty page, for servers before #404), and raises `EventStoreError` if the
  cursor does not advance. `stream_exists` reads one event instead of the
  whole stream.
- `MemoryEventStoreClient` now numbers global nonces from 1, like the store,
  and ignores a caller-supplied `global_nonce` (it could duplicate one and
  make paging skip or repeat an event). `read_events(from_version=N)` starts
  at version N inclusive, like the store (it started at N + 1), and an
  unknown stream reads empty instead of raising.

### Fixed (v0.17.0 release review, #349)

- Filter before decode (ADR-027): `GrpcEventStoreClient.subscribe` takes
  `event_types`; events of other types (after upcasting) are yielded
  undecoded. `SubscriptionCoordinator` passes each track its projections'
  subscribed types, so an evolved type with no upcaster no longer halts
  projections that do not handle it. New `Upcasters.target()`.
- A ProcessManager that failed live event N and recovered it after a
  re-plan with head H >= N (a reconnect) stayed catching up until H+1, so its
  `process_pending()` never ran. Its track now goes live, and wakes its
  drains, once it has delivered H (#391).
- A projection rebuilt while its old track runs no longer widens that
  track's type filter. One membership test now gates dispatch, filtering and
  drain eligibility.

### Fixed (projection failures are never stepped over, syntropic137#1696)

- `SubscriptionCoordinator` no longer checkpoints past an event a projection
  failed to apply. A projection whose `handle_event` returns `FAILURE` or
  raises is held below the event and fed it again on a track of its own, with
  backoff (1s doubling to 30s); every other projection keeps consuming
  (ADR-026). Before, the failure was logged and the projection's next event
  checkpointed past it, losing the event silently. A held ProcessManager runs
  no `process_pending()` until it has applied the event; an in-flight drain is
  cancelled when the hold lands. A retry pending across a rebuild is dropped.
- New `ProjectionHandlerFailedError` (exported from `event_sourcing` and
  `event_sourcing.subscriptions`). `dispatch_event()` now raises it (an
  `ExceptionGroup` of them when several projections fail one event) after
  offering the event to every projection.
- New `SubscriptionCoordinator.held_projections`; `is_healthy` is False while
  any projection is held.

### Added (gateway credentials, #302)

- Gateway credentials (ADR-024, #302): `GrpcEventStoreClient` and
  `EventStoreClientFactory.create_grpc_client` take `auth` (`BasicAuth`,
  `BearerToken`, `TokenProviderAuth` with a sync or async provider, e.g.
  `SharedToken`), `tls` (`True` or `TlsConfig`) and
  `allow_insecure_credentials`. The `authorization` header is added to every
  call, including `subscribe()`. Addresses accept `http://` and `https://`.
- Credentials over plaintext to a non-loopback host raise `ClientConfigError`
  unless `allow_insecure_credentials=True`. Secrets are redacted from `repr`.
- `EventStoreAuthenticationError` (an `EventStoreError`) for gRPC
  `UNAUTHENTICATED`, including from a failing token provider.
### Breaking

Cross-language event envelope (ADR-027, #382):

- **The real event version is written.** `GrpcEventStoreClient` writes the
  event class's `schema_version` as `event_version` (it always wrote 1). The
  `@event` version string stays descriptive metadata.
- **Decoding is by `(event_type, event_version)`.** The registry keeps every
  registered `schema_version`. A stored event of a registered type at a
  version with no class, after upcasting, raises `UnknownEventVersionError`
  instead of being validated against another version.
- **A payload the registered model rejects raises `EventPayloadError`.** It
  fell back to `GenericDomainEvent` silently. `GrpcEventStoreClient(...,
  on_invalid_payload="generic")` keeps the old fallback, with a warning, as a
  migration aid.
- Non-JSON content types raise `UnsupportedContentTypeError`; a payload that is
  not a JSON object raises `EventPayloadError`.
- All decode errors are `EventDecodeError`, an `UndecodableEventError`: a
  `SubscriptionCoordinator` halts at the event (ADR-026) instead of retrying.
- `@event` raises `ValueError` for a `schema_version` that is not an int >= 1.

Migration: none for stored data. Python streams were all written as v1, and
v1 is still the default `schema_version`. Payloads written by the TypeScript
SDK <= 0.17 (with `eventType`/`schemaVersion` keys) now validate against
strict models: readers drop those keys.

### Added

- `Upcasters` (`register(type, from, to, fn)`, `rename(from_type, from, to_type, to, fn)`),
  same semantics as the Rust SDK; `upcasters=` on `GrpcEventStoreClient` and
  `EventStoreClientFactory.create_grpc_client`.
- `decode_event`, `DecodedEvent`, `resolve_event_class(type, version)`,
  `registered_event_versions(type)`, `ENVELOPE_ECHO_KEYS`.
- `EventMetadata.event_version`, `stored_event_type`, `stored_event_version`
  (set on read; `event_type`/`event_version` are the decoded values).
- Errors: `EventDecodeError`, `UnknownEventTypeError`,
  `UnknownEventVersionError`, `EventPayloadError`,
  `UnsupportedContentTypeError`, `UpcastError`.
- Golden fixture tests decoding the TypeScript, Python, Rust and legacy
  TypeScript encodings into strict models (`tests/unit/test_xlang_golden.py`).

### Changed

- A `GenericDomainEvent` is written without its `event_type` attribute in the
  payload (the type is metadata).
- A `GenericDomainEvent` read from the store keeps the version it was read at
  (`event.event_version`) and is written back at that version.

## [0.14.0] - 2026-04-16

### Added

- `HistoricalPoller.process()` now accepts an `is_replay: bool = False` keyword
  argument. The base class sets `is_replay=True` when invoking `process()` on
  the cold-start path (events that survived the `_started_at` timestamp fence
  during the first poll for a source). Subclasses may use this flag to mark
  events as unprimed so downstream consumers skip side-effectful work such as
  trigger evaluation. Default value preserves Liskov compatibility for
  existing subclasses.

### Changed

- Cold-start branch in `HistoricalPoller.poll()` now passes `is_replay=True` to
  `process()`. Warm-start (steady-state) polling continues to pass the default
  `is_replay=False`.

### Context

Consumers of the GitHub Events API pattern (and similar re-delivering APIs)
need a signal that the current batch is cold-start replay, separate from the
timestamp fence that only kicks in on the first poll per source. Relying on
mutable state such as a `primed_sources` set proved race-prone: the framework
primes the source before calling `process()`, so any "am I primed yet?"
check observed from inside the subclass was always True. The `is_replay`
kwarg delivers the signal directly from `poll()` to `process()` without
going through mutated framework state.
