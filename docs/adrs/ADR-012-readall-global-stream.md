# ADR-012: ReadAll RPC for Global Event Stream

**Status:** Accepted
**Date:** 2025-12-05
**Decision Makers:** Platform Team
**Technical Story:** Projection catch-up fails due to unreliable event reading

## Context

The event-sourcing platform currently provides three core operations:

1. **Append** - Write events to a stream
2. **ReadStream** - Read events from a specific aggregate stream
3. **Subscribe** - Live streaming subscription for new events

However, there is no proper **batch read from global position** operation. The Python SDK's `read_all_events_from()` method currently abuses the `Subscribe` RPC with timeout/keepalive heuristics to simulate batch reading:

```python
# Current problematic implementation
async for response in self._stub.Subscribe(request):
    if not response.HasField("event"):
        consecutive_keepalives += 1
        if consecutive_keepalives >= 10:  # Fragile heuristic!
            break  # Assume end of historical events
```

This approach fails in production:
- Keepalive timing varies under load
- No explicit end-of-batch signal
- Timeout-based detection is unreliable
- Results in missed events during projection catch-up

## Decision

Add a proper `ReadAll` RPC to the EventStore service that:

1. **Returns paginated events** from a global position
2. **Provides explicit `is_end` flag** to indicate end of batch
3. **Is separate from Subscribe** which remains for live streaming only

### Proto Definition

```protobuf
message ReadAllRequest {
  string tenant_id         = 1;
  uint64 from_global_nonce = 2;  // Inclusive
  uint32 max_count         = 3;  // Page size
  bool   forward           = 4;  // Direction
}

message ReadAllResponse {
  repeated EventData events      = 1;
  bool is_end                    = 2;
  uint64 next_from_global_nonce  = 3;
}

service EventStore {
  rpc ReadAll (ReadAllRequest) returns (ReadAllResponse);
}
```

### Operation Semantics

| Parameter | Default | Description |
|-----------|---------|-------------|
| `from_global_nonce` | 0 | Start position (inclusive) |
| `max_count` | 100 | Events per page (max: 1000) |
| `forward` | true | Ascending order |

| Response Field | Description |
|----------------|-------------|
| `events` | Events in requested order |
| `is_end` | True if no more events exist |
| `next_from_global_nonce` | Position for next page |

## Consequences

### Positive

- **Reliable catch-up:** Explicit pagination eliminates heuristic-based detection
- **Standard pattern:** Matches EventStoreDB, Marten, Axon approaches
- **Clear semantics:** Batch vs stream operations are distinct
- **Testable:** Deterministic behavior, easy to test

### Negative

- **Additional RPC:** One more endpoint to maintain
- **Proto regeneration:** All SDKs need stub updates
- **Migration:** Existing code using `Subscribe` for catch-up needs updating

### Neutral

- **Performance:** Similar to `Subscribe` for small batches
- **Complexity:** Simple SQL/memory query implementation

## Alternatives Considered

### 1. Improve Subscribe with End Signal

Add an explicit "end of historical" message to Subscribe:

```protobuf
message SubscribeResponse {
  oneof content {
    EventData event = 1;
    bool caught_up = 2;  // Signal caught up to live
  }
}
```

**Rejected because:**
- Mixes concerns (batch + live in one stream)
- Complicates client implementation
- Doesn't match industry patterns

### 2. Longer Timeouts / Better Heuristics

Increase timeout or improve keepalive detection:

```python
max_consecutive_keepalives = 50  # Longer wait
timeout_seconds = 30.0  # More time
```

**Rejected because:**
- Still fundamentally fragile
- Slower catch-up in best case
- Still fails under varying load

### 3. Separate Historical Query Service

Create a dedicated query service for historical reads:

**Rejected because:**
- Over-engineering for this use case
- Additional infrastructure
- `ReadAll` on existing service is sufficient

## Implementation Notes

### Postgres Query

```sql
SELECT * FROM events
WHERE tenant_id = $1
  AND global_nonce >= $2
ORDER BY global_nonce ASC
LIMIT $3
```

**Index:** `(tenant_id, global_nonce)` - likely already exists

### Catch-up Pattern

```python
async def catch_up(from_position: int):
    while True:
        events, is_end, next_pos = await client.read_all(
            from_global_nonce=from_position,
            max_count=100,
        )

        for event in events:
            await process(event)

        if is_end:
            break  # Explicit end, not heuristic

        from_position = next_pos
```

## Related Decisions

- **ADR-007:** Event Store Architecture - Established gRPC service pattern
- **ADR-009:** CQRS Pattern - Projections depend on reliable event reading

## References

- [EventStoreDB $all stream](https://developers.eventstore.com/server/v21.10/streams.html#the-all-stream)
- [Marten Event Store](https://martendb.io/events/querying.html)
- [Axon Framework Event Processor](https://docs.axoniq.io/reference-guide/axon-framework/events/event-processors)
