# Changelog

All notable changes to the Python Event Sourcing SDK will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
