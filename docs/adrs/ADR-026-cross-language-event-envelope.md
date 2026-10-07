# ADR-026: Cross-Language Event Envelope

**Status:** Accepted
**Date:** 2026-10-06
**Deciders:** NeuralEmpowerment
**Relates to:** ADR-007 (Event Versioning and Upcasters), ADR-023 (Event Type Registry), #371

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
| `event_version` | Schema version of `event_type`, starting at 1 (TS `schemaVersion`, Rust `EVENT_VERSION`). `0` is proto3 "unset" and readers MUST treat it as 1. |
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

Readers ignore unknown payload fields. In particular they MUST tolerate
`eventType` and `schemaVersion` keys in the payload (see Known deviations).

### Golden fixtures

`event-sourcing/rust/tests/fixtures/xlang/{typescript,python}.json` hold
protobuf `AppendRequest` bytes captured from the real SDK encoders
(`tests/xlang/ts_peer.cjs`, `tests/xlang/py_peer.py`). The Rust tests decode
them, re-encode the same events, and require byte-identical metadata and
JSON-equal payloads. `make -C event-sourcing/rust test-xlang` additionally
runs both directions against a live event store (TS/Python write, Rust
reads; Rust writes, TS/Python read).

## Known deviations in existing SDKs

Recorded so they can be fixed without changing the canonical envelope:

- **TypeScript payload echoes class fields.** `BaseDomainEvent.toJson()`
  serializes the instance, so `eventType` and `schemaVersion` (class fields)
  appear in the payload: `{"eventType":"MoneyDeposited","schemaVersion":1,"amount":125}`.
  They duplicate metadata. Readers ignore them; the Python reader's
  `extra="forbid"` models currently reject them and fall back to
  `GenericDomainEvent`. Rust events must not use `deny_unknown_fields` when
  reading TS streams.
- **Python always writes `event_version = 1`.** `GrpcEventStoreClient`
  ignores `DomainEvent.schema_version`, so Python cannot yet write a v2
  event. Python payloads use `json.dumps` defaults (`", "` separators,
  `\u` escapes), which is valid and only differs in formatting.
- **TypeScript writes `event_version = 0`** for an event without
  `schemaVersion`; readers read it as 1.
- **Neither TS nor Python checks `aggregate_type` on load.** Rust does: a
  stream whose first event has another aggregate type is an error.

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
