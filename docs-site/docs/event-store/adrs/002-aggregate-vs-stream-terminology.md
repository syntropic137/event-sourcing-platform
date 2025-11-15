# ADR 002: Aggregate vs Stream Terminology

## Status
Accepted

## Context
We needed to choose consistent terminology for our event store entities. Two main approaches existed:

**Option A: Stream-Centric Terminology**
- `stream_id` - Identifier for a stream of events
- `stream_type` - Category/type of stream
- `stream_version` - Version within a stream
- Common in existing event stores (EventStoreDB)

**Option B: Aggregate-Centric Terminology**
- `aggregate_id` - Identifier for an aggregate instance
- `aggregate_type` - Type of aggregate
- `aggregate_nonce` - Sequence number within aggregate
- Domain-Driven Design aligned, Axon Framework style

## Decision
We chose **Option B: Aggregate-Centric Terminology** for DDD alignment.

## Rationale

### Why Aggregate-Centric is Better

1. **Domain-Driven Design Alignment**
   - Matches DDD concepts of aggregates and bounded contexts
   - `aggregate_id` clearly identifies the consistency boundary
   - Natural fit for event-sourced aggregates

2. **Clearer Semantics**
   - `aggregate_nonce` vs `stream_version` is more precise
   - Nonce implies cryptographic/security context (like blockchain)
   - Aggregate terminology is more business-focused

3. **Axon Framework Compatibility**
   - Matches Axon's terminology exactly
   - Easier interop with Axon ecosystem
   - Familiar to DDD practitioners

### Terminology Mapping

| Legacy (Stream) | Modern (Aggregate) | Description |
|----------------|-------------------|-------------|
| `stream_id` | `aggregate_id` | Instance identifier (e.g., "Order-123") |
| `stream_type` | `aggregate_type` | Aggregate class/type |
| `stream_version` | `aggregate_nonce` | Per-aggregate sequence |
| `global_nonce` | `global_nonce` | Global sequence |

## Consequences

### Positive
- ✅ DDD and Axon Framework alignment
- ✅ Clearer business semantics
- ✅ Better for event-sourced aggregates
- ✅ Future-proof for complex domain models

### Negative
- ❌ Different from some existing event stores
- ❌ Learning curve for stream-centric developers
- ❌ Migration effort from legacy terminology

### Migration Strategy
1. **Proto Layer**: Updated to use aggregate terminology
2. **Implementation**: Updated Rust backends to use aggregate terms
3. **Documentation**: All docs updated to use aggregate terminology
4. **Client SDKs**: Will use aggregate terminology consistently

## Alternatives Considered

### Stream-Centric Terminology
- Rejected because it's less DDD-aligned
- More generic but less semantically rich
- Doesn't convey the consistency boundary concept as clearly

## Implementation Notes

### Proto Changes
- `stream_id` → `aggregate_id`
- `stream_type` → `aggregate_type`
- `stream_version` → `aggregate_nonce`
- `global_nonce` → `global_nonce`

### Code Changes
- Updated all Rust implementations
- Memory and Postgres backends use aggregate terminology
- Client-proposed nonce validation implemented

## References
- Domain-Driven Design by Eric Evans
- Axon Framework documentation
- Event Modeling methodology
