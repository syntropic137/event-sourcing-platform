# ADR-023: Event Type Registry

**Status:** Accepted
**Date:** 2026-04-07
**Deciders:** NeuralEmpowerment
**Relates to:** ADR-007 (Event Versioning), ADR-010 (Decorator Patterns)

## Context

When the gRPC event store client deserializes events from the wire, it receives:
- **Payload**: opaque JSON bytes (the event fields)
- **Metadata**: `EventMetadata` including `event_type` (e.g., `"WorkflowCreated"`)

Prior to this ADR, the Python gRPC client always created `GenericDomainEvent(**payload_dict)` regardless of event type. This meant:

1. **Aggregates could not rehydrate** — `AggregateRoot.apply_event()` calls `_get_event_type(event)` which checks `hasattr(event, "event_type")`. Since `DomainEvent.event_type` is a `ClassVar` (not an instance attribute), `GenericDomainEvent` instances returned `"GenericDomainEvent"` as the type, causing all event handlers to be skipped silently.

2. **Moving `event_type` into metadata** (PR #271) fixed projection adapters that call `model_validate()` on concrete `DomainEvent` subclasses (which use `extra="forbid"` and would reject an `event_type` field in the payload). But this broke aggregate rehydration, which only receives `envelope.event` without metadata.

3. **Putting `event_type` back in the payload** would re-break projections. The two consumers need the type information in different places.

The root cause is that the SDK has no mechanism to resolve concrete event types from the wire format. The `@event` decorator stores metadata on classes but doesn't populate any registry for the deserialization path.

## Decision

Implement a **global event type registry** that:

1. Is automatically populated by the `@event` decorator at import/decoration time
2. Is consulted by the gRPC client's `_proto_to_envelope()` to resolve concrete Python classes
3. Falls back gracefully to `GenericDomainEvent` (with `event_type` preserved as an instance attribute) for unknown/unregistered event types
4. Is implemented consistently across all SDK languages

### Registry Specification

```
Registration:  @event("MyEvent", "v1") → registry["MyEvent"] = MyEventClass
Lookup:        _proto_to_envelope(meta.event_type="MyEvent") → MyEventClass.model_validate(payload)
Fallback:      _proto_to_envelope(meta.event_type="Unknown") → GenericDomainEvent(**payload, event_type="Unknown")
```

The registry is:
- **No deletions at runtime** — entries are never removed once registered; later registrations for the same `event_type` overwrite the existing entry
- **Global per process** — one registry shared across all aggregates and projections
- **Populated at import time** — when Python modules containing `@event`-decorated classes are imported, they auto-register

### Python SDK

**Module:** `event_sourcing.decorators.events`

```python
_EVENT_TYPE_REGISTRY: dict[str, type[DomainEvent]] = {}

def get_event_type_registry() -> dict[str, type[DomainEvent]]:
    """Return a copy of the global event type registry."""
    return dict(_EVENT_TYPE_REGISTRY)
```

In `@event` decorator (via `_try_register_event_type()`):
```python
try:
    if issubclass(cls, DomainEvent):
        _EVENT_TYPE_REGISTRY[event_type] = cls
except TypeError:
    pass  # Guard: issubclass() raises if cls is not a class
```

In `GrpcEventStoreClient._proto_to_envelope()`:
```python
concrete_cls = resolve_event_type(event_type_str)
cleaned = {k: v for k, v in payload_dict.items() if k != "event_type"}
if concrete_cls is not None:
    try:
        event = concrete_cls.model_validate(cleaned)  # strip event_type to avoid extra="forbid" clash
    except Exception:
        event = GenericDomainEvent(**cleaned, event_type=event_type_str)
else:
    event = GenericDomainEvent(**cleaned, event_type=event_type_str)
```

### TypeScript SDK

The `EventSerializer.eventRegistry` Map already exists with `registerEvent()`. Changes:

1. `@Event` decorator auto-calls `EventSerializer.registerEvent(eventType, constructor)`
2. `EventSerializer.deserialize()` returns a generic event object (with `eventType` preserved) instead of throwing on unknown types

### Rust SDK

Planned approach (deferred until gRPC client is implemented):
- `#[event("MyEvent", "v1")]` procedural macro
- `inventory::submit!` crate for link-time registration
- Registry trait with `resolve(event_type: &str) -> Option<Box<dyn DomainEvent>>`

## Consequences

### Positive

- **Aggregates receive concrete types** — event handlers match correctly, eliminating the entire class of rehydration failures
- **Projections unaffected** — `event_type` stays in metadata, not in the payload; projection adapters continue to use `envelope.metadata.event_type`
- **Zero configuration** — applications get the registry for free just by using `@event`
- **Graceful degradation** — unknown event types still work via `GenericDomainEvent` with `event_type` accessible
- **Cross-SDK consistency** — same pattern across Python, TypeScript, and (future) Rust

### Negative

- **Import order matters** — event classes must be imported before the gRPC client reads events. In practice this is always true since aggregates import their event classes, and aggregates are instantiated before event reading begins.
- **Global mutable state** — the registry is process-global. This is acceptable because it's append-only and populated deterministically at import time.

### Risks

- **Duplicate registrations** — two classes decorated with the same `event_type` string will silently overwrite each other. This is intentional for event versioning (the latest version wins), but could mask errors if two unrelated events share a name. Mitigated by the existing `@event` decorator validation that checks `event_type` matches the class attribute.
