# SDK Feature Request: Add `readAllEventsFrom()` / `read_all_events_from()` for Projections

## Summary

Both Python and TypeScript SDKs are missing a critical method for building projections: reading all events from a global position. The Rust event store backend already supports this via the `Subscribe` RPC, but the SDKs don't expose it.

## Problem

**Current State:**
- ✅ SDKs can read events from a **single aggregate stream** (`readEvents(streamName)`)
- ❌ SDKs **cannot** read events **across all aggregates** for projections
- ❌ No way to rebuild projections from history
- ❌ No way to catch up from a checkpoint

**Impact:**
- Cannot implement CQRS read models / projections
- Cannot build analytics dashboards
- Cannot replay events for projection rebuild
- Projections must be stateful services with complex subscription management

## Existing Infrastructure

The Rust backend already has this! See `eventstore.proto`:

```protobuf
message SubscribeRequest {
  string tenant_id                = 1;
  string aggregate_id_prefix      = 2;  // optional filter
  uint64 from_global_nonce        = 3;  // 0 => from beginning ✅
}

message SubscribeResponse {
  EventData event                 = 1;
}

service EventStore {
  rpc Subscribe (SubscribeRequest) returns (stream SubscribeResponse);
}
```

**Key field:** `from_global_nonce` enables checkpoint-based reading!

## Proposed Solution

### TypeScript SDK

**File:** `event-sourcing/typescript/src/client/event-store-client.ts`

**Add to `EventStoreClient` interface:**

```typescript
export interface EventStoreClient {
  // ... existing methods ...
  
  /**
   * Read all events from a global position (for projections/catch-up).
   * 
   * This method enables projections to rebuild state by reading all events
   * in order from a checkpoint.
   * 
   * @param fromGlobalNonce - Global position to read from (inclusive), 0 for beginning
   * @param limit - Maximum number of events to return (for batching)
   * @returns Promise resolving to array of event envelopes in global order
   */
  readAllEventsFrom(
    fromGlobalNonce: number,
    limit?: number
  ): Promise<EventEnvelope[]>;
}
```

**Implementation:**

1. **Memory Client** (`event-sourcing/typescript/src/integrations/memory-event-store.ts`):
   - Assign `globalNonce` on append
   - Collect events from all streams
   - Sort by `globalNonce`
   - Filter and limit

2. **gRPC Client** (`event-sourcing/typescript/src/integrations/grpc-event-store.ts`):
   - Use `Subscribe` RPC
   - Set `fromGlobalNonce` parameter
   - Handle streaming response
   - Collect up to `limit` events

### Python SDK

**File:** `event-sourcing/python/src/event_sourcing/client/event_store.py`

**Add to `EventStoreClient` Protocol:**

```python
class EventStoreClient(Protocol):
    # ... existing methods ...
    
    async def read_all_events_from(
        self,
        after_global_nonce: int = 0,
        limit: int = 100,
    ) -> list[EventEnvelope[DomainEvent]]:
        """
        Read all events from a global position (for projections/catch-up).

        This method enables projections to rebuild state by reading all events
        in order from a checkpoint.

        Args:
            after_global_nonce: Global position to read from (exclusive)
            limit: Maximum number of events to return (for batching)

        Returns:
            List of event envelopes in global order

        Raises:
            EventStoreError: If reading fails
        """
        ...
```

**Implementation:**

1. **Memory Client** (`event-sourcing/python/src/event_sourcing/client/memory.py`):
   - ✅ **Already implemented** (see PR/commit)
   - Assigns `global_position` on append
   - Collects and sorts events

2. **gRPC Client** (`event-sourcing/python/src/event_sourcing/client/grpc_client.py`):
   - Use `Subscribe` RPC
   - Set `from_global_nonce` parameter
   - Handle gRPC stream
   - Return list up to limit

## Naming Conventions

| Language   | Method Name              | Parameter Name        | Notes |
|------------|--------------------------|-----------------------|-------|
| TypeScript | `readAllEventsFrom()`    | `fromGlobalNonce`     | camelCase |
| Python     | `read_all_events_from()` | `after_global_nonce`  | snake_case |
| Protobuf   | `Subscribe`              | `from_global_nonce`   | snake_case |

**Parameter Semantics:**
- TypeScript/Proto: `from` is **inclusive** (start reading at this position)
- Python: `after` is **exclusive** (start reading after this position)
- Both support `0` for "from the beginning"

## Use Cases

### 1. Projection Rebuild
```typescript
// Rebuild projection from scratch
const events = await client.readAllEventsFrom(0, 1000);
for (const event of events) {
  await projection.handleEvent(event);
}
```

### 2. Projection Catch-Up
```python
# Continue from checkpoint
last_processed = await projection.get_checkpoint()
events = await client.read_all_events_from(last_processed, 100)
for event in events:
    await projection.handle_event(event)
```

### 3. Analytics Dashboard
```python
# Read recent events for metrics
recent_events = await client.read_all_events_from(
    after_global_nonce=global_nonce - 1000,
    limit=1000
)
metrics = calculate_metrics(recent_events)
```

## Acceptance Criteria

- [ ] TypeScript SDK: `readAllEventsFrom()` added to interface
- [ ] TypeScript SDK: Memory client implementation
- [ ] TypeScript SDK: gRPC client implementation  
- [ ] Python SDK: `read_all_events_from()` added to Protocol
- [ ] Python SDK: Memory client implementation ✅
- [ ] Python SDK: gRPC client implementation
- [ ] Unit tests for both implementations (each SDK)
- [ ] Integration tests with Rust event store backend
- [ ] Documentation with examples
- [ ] Update SDK README files

## Related Issues

- Event sourcing CQRS pattern implementation
- Projection/read model infrastructure
- Analytics and monitoring dashboards

## Priority

**High** - This is a fundamental CQRS capability. Without it, projections require complex workarounds or stateful subscription management.

## Additional Context

Discovered while implementing projection wiring for agentic prototyping POC. The lack of this method blocked projection rebuild and catch-up logic.

**Current workaround:** Manually iterate all aggregate IDs and call `readEvents()` for each - inefficient and doesn't maintain global order.

---

**Labels:** enhancement, SDK, TypeScript, Python, CQRS, projections  
**Milestone:** v1.1.0 (SDK Feature Parity)

