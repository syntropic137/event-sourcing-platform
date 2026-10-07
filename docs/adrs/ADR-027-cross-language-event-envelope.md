# ADR-027: Cross-Language Event Envelope

**Status:** Accepted
**Date:** 2026-10-06
**Deciders:** NeuralEmpowerment
**Relates to:** ADR-007 (Event Versioning and Upcasters), ADR-023 (Event Type Registry), #371, #382

## Context

The event store is language-neutral: `EventData` is `EventMetadata` plus opaque
`payload` bytes. How an SDK maps a domain event onto those fields was never
written down. The TypeScript and Python SDKs converged on one encoding; the
Rust SDK (pre-release) did not:

- it stored the whole serde enum (`{"OrderSubmitted":{...}}`) instead of the
  flat body, and ignored `event_type`/`event_version` on load;
- its aggregate type defaulted to `std::any::type_name` (a module path that
  changes on refactor and that no other SDK can produce).

A stream is written once and read forever, by any service, in any language.
The encoding is a one-way door, so it is fixed here and verified by golden
tests built from the real TypeScript and Python encoders.

## Decision

Every SDK writes and reads exactly this envelope. Field names are
`eventstore.v1.EventMetadata` / `EventData`.

### Event fields

| Field | Rule |
|-------|------|
| `payload` | UTF-8 JSON **object** containing only the event's own fields: `{"amount":125,"note":"..."}`. Never wrapped in a type tag, never a scalar, array or `null`. Formatting (whitespace, key order, `\u` escapes) is not significant; readers compare JSON values, not bytes. |
| `event_type` | Stable event name, e.g. `MoneyDeposited`. Non-empty printable ASCII without spaces. PascalCase past tense is the convention (TS `@Event('MoneyDeposited', 'v1')`, Python `event_type: ClassVar[str]`, Rust `EventSchema::EVENT_TYPE`). |
| `event_version` | Schema version of `event_type`, starting at 1 (TS `schemaVersion`, Python `schema_version: ClassVar[int]`, Rust `EVENT_VERSION`). The `@Event`/`@event` version string is descriptive metadata, not this value. `0` is proto3 "unset": readers MUST treat it as 1, and writers write 1 for an event without a version. |
| `content_type` | `application/json`. Readers treat empty as JSON and MUST fail with a typed error on anything else. |
| `content_schema` | Empty (reserved). |
| `aggregate_type` | Stable aggregate name, e.g. `Account`: an ASCII letter followed by letters, digits, `_` or `.`. Never derived from a language type path. Must not contain `-`: the TS and Python repositories build stream names as `{aggregate_type}-{aggregate_id}` and split on the first `-`. Same value in `AppendRequest.aggregate_type`. |
| `aggregate_id` | Stream id. May contain `-`. |
| `aggregate_nonce` | 1-based position in the stream, proposed by the client. |
| `event_id` | Lowercase hyphenated UUID, unique per event. |
| `tenant_id` | Tenant of the stream; equal to `AppendRequest.tenant_id`. |
| `timestamp_unix_ms` | Client event time, Unix milliseconds. |
| `correlation_id`, `causation_id`, `actor_id` | Empty string when absent. |
| `headers` | Free-form string map; empty by default. |
| `payload_sha256` | Empty (optional integrity hash). |
| `recorded_time_unix_ms`, `global_nonce` | `0` on write; assigned by the store. |

### Reading

1. Check `content_type` (empty or `application/json`).
2. Normalize `event_version` (`0` becomes `1`).
3. Run the upcaster chain on `(event_type, event_version, payload JSON)`.
   Each step maps one `(type, version)` to a newer version or (rename) to
   another type; steps chain until none matches.
4. Decode by dispatching on the resulting `(event_type, event_version)`.
5. An event the reader cannot decode is a **typed error** (unknown type,
   unknown version, payload mismatch), never silently skipped. A projection
   that does not care about a type filters it by `event_type` before
   decoding.

Readers ignore unknown payload fields, unless the application opts into
strict models (Python's `DomainEvent` defaults to `extra="forbid"`): there an
unknown field is a payload-mismatch error, so a field that strict readers must
tolerate needs a version bump and an upcaster.

Readers MUST tolerate the keys `eventType` and `schemaVersion` (written into
every payload by TypeScript SDK <= 0.17), and Python also `event_type`
(older Python producers). Streams written before #382 stay readable without
migration:

- TypeScript drops `eventType`/`schemaVersion` (`DomainEvent` members, never
  event data) before upcasting and decoding.
- Python passes the body to upcasters unchanged, then drops the three keys
  the selected model does not declare (by name or alias) right before
  validation, so strict models accept legacy payloads. A `GenericDomainEvent`
  keeps them (the schema is unknown).
- Rust passes the body through unchanged; serde ignores unknown fields by
  default, so legacy payloads decode. A Rust event that may read pre-#382 TS
  streams MUST NOT use `#[serde(deny_unknown_fields)]`, and an upcaster
  step for such a stream sees the keys.

### SDK mapping

| | Rust | TypeScript | Python |
|---|---|---|---|
| Version written | `EVENT_VERSION` | `schemaVersion` (missing or 0 written as 1) | `schema_version` ClassVar (default 1) |
| Decoder | `event_enum!` / `RecordedEvent::decode` | `EventSerializer` registry keyed by `(eventType, schemaVersion)`, filled by `@Event` | registry keyed by `(event_type, schema_version)`, filled by `@event` |
| Upcasters | `Upcasters::new().register(..).rename(..)`; `with_upcasters` on repository and projection runner | `new Upcasters().register(..).rename(..)`; `upcasters` option of `GrpcEventStoreAdapter` / `EventStoreClientFactory.createGrpcClient` | `Upcasters().register(..).rename(..)`; `GrpcEventStoreClient(upcasters=...)` |
| Typed errors | `Error::UnknownEventType`, `UnknownEventVersion`, `EventDecode`, `UnsupportedContentType`, `Upcast` | `UnknownEventTypeError`, `UnknownEventVersionError`, `EventPayloadError`, `UnsupportedContentTypeError`, `UpcastError` (all `EventDecodeError`) | same names (all `EventDecodeError`, an `UndecodableEventError`, so a `SubscriptionCoordinator` halts instead of retrying) |
| Stored type/version exposed | `RecordedEvent::event_type`/`event_version` | `metadata.storedEventType`/`storedEventVersion`; `event.eventType`/`schemaVersion` are the decoded ones | `metadata.stored_event_type`/`stored_event_version`; `metadata.event_type`/`event_version` are the decoded ones |

The TypeScript and Python clients decode eagerly on every read
(`readEvents`/`read_events`, `readAll`/`read_all`, `subscribe`), where Rust
hands projections a raw `RecordedEvent`. Their equivalent of "not decoded" is
the generic event of ADR-023: a type with **no** registered class at any
version is returned as a generic event carrying the decoded type, version and
every payload field, never dropped, so a projection can filter it by type
(strict decode: `requireRegistered` / `require_registered`). A type that is
registered, but not at the event's version after upcasting, is always
`UnknownEventVersionError`, never handed to a class of another version.
Python raises `EventPayloadError` when the registered model rejects the
payload (`GrpcEventStoreClient(on_invalid_payload="generic")` restores the
pre-#382 fallback as a migration aid); TypeScript classes have no schema, so
for TS a payload mismatch is only a payload that is not a JSON object.

Python filters before decoding on subscriptions: `subscribe(event_types=...)`
yields an event whose type after upcasting (known from the upcaster chain
without running it) is not in the set undecoded, as a `GenericDomainEvent`
with no payload fields, carrying its position and type. `SubscriptionCoordinator`
passes each track the types its current projections subscribe to (all when one of
them subscribes to every type), so an event no projection on the track handles
is skipped and checkpointed past like any other skipped type, and cannot halt
the track. An event a projection handles still raises, and halts (ADR-026).

Rust's `ProjectionRunner` does the same: it asks `handles()` with the type
after upcasting (`Upcasters::target`, which runs no step), skips and
checkpoints past an event the projection does not handle without upcasting
or parsing it, and upcasts only a handled event (or a rename cycle, which
has no target), where a failure stops the runner (#396).

### Golden fixtures

`event-sourcing/rust/tests/fixtures/xlang/` holds protobuf `AppendRequest`
bytes from the real encoders: `typescript.json` (`tests/xlang/ts_peer.cjs`),
`python.json` (`tests/xlang/py_peer.py`), `rust.json` (the Rust repository),
regenerated deterministically by `make -C event-sourcing/rust
test-xlang-fixtures`, and the frozen `typescript-legacy.json` (TypeScript SDK
0.17, payloads echo `eventType`/`schemaVersion`). Each holds a v1, v1, v2
stream.

- The Rust tests require byte-identical metadata and JSON-equal payloads
  between the Rust encoder and the TS and Python fixtures, check that the
  current encoders' payloads hold exactly the event's fields, and decode all
  four fixtures.
- The TypeScript (`tests/xlang-golden.test.ts`) and Python
  (`tests/unit/test_xlang_golden.py`, strict models) suites decode all four,
  upcast the v1 event to a v2 class, and check their own fixture is what
  their encoder writes today.
- `make -C event-sourcing/rust test-xlang` runs all six directions (TS, Python
  and Rust each writing, each other SDK reading) against a live event store,
  asserting the decoded class, the reported and stored version, and the
  payload.

Python payloads use `json.dumps` defaults (`", "` separators, `\u` escapes);
that is valid and only differs in formatting.

## Known deviations in existing SDKs

The TypeScript and Python deviations recorded with the first version of this
ADR (payload echoing `eventType`/`schemaVersion`, Python always writing
`event_version = 1`, TS writing `0`, readers not dispatching on
`event_version`) were fixed in #382. Remaining:

- **Neither TS nor Python checks `aggregate_type` on load.** Rust does: a
  stream whose first event has another aggregate type is an error.
- **A Python event already stored at `event_version = 1` by a class whose
  `schema_version` was later bumped** (Python ignored `schema_version` on
  write before #382) is read as v1: register the old shape at v1 or an
  upcaster from v1. Only the version label was wrong; the payload is intact.

## Consequences

- Any SDK reads any stream; a service can be rewritten in another language
  without migrating data.
- Renaming a Rust module, type or enum variant never changes stored data;
  only `AGGREGATE_TYPE`, `EVENT_TYPE` and `EVENT_VERSION` do, and they are
  explicit constants.
- Schema evolution is explicit: bump the version and register an upcaster.
- The Rust SDK change is breaking (pre-release): events are structs with
  `EventSchema`, grouped with `event_enum!`; aggregates declare
  `AGGREGATE_TYPE`.
