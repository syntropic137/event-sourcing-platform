# ADR-013: Subscribe Cursor Advancement After Yield

**Status**: Accepted ✅
**Date**: 2025-12-05
**Author**: AI Pair Programming Session

## Context

The `Subscribe` gRPC method provides real-time event streaming with two phases:
1. **Replay Phase**: Delivers historical events from a starting position
2. **Live Phase**: Polls for and delivers new events as they're written

During investigation of missing events in projection subscribers, we discovered that the cursor (position tracker) was being advanced **before** events were yielded to the client.

### The Bug

In the Live phase of `eventstore-backend-postgres`:

```rust
// Lines 748-764 (before fix)
for row in rows.iter() {
    let row_cursor = row.get::<i64, _>("global_nonce");
    cursor = cursor.max(row_cursor);  // ← Advances to max position IMMEDIATELY
    if let Ok(event) = row_to_event(row) {
        items.push(event);
    }
}
let event = items.first().cloned();  // Only first event yielded this iteration
```

**Problem**: If the gRPC stream is interrupted between:
- Cursor being advanced to position N+1
- Event at position N being delivered to client

Then event N is lost because the client will resume from N+1.

### Event Sourcing Best Practice

From Oskar Dudycz's authoritative guidance on EventStoreDB subscriptions:

> "The typical flow for catch-up subscriptions:
> 1. Load the last processed event position.
> 2. If it exists, subscribe to the next event.
> 3. **Get a notification about the event and process it.**
> 4. **Store position of that event.**"

The checkpoint (cursor) should only advance **after** the event is successfully processed/delivered.

## Decision

**Fix the `Subscribe` implementation to only advance the cursor after an event has been yielded.**

### Implementation

The cursor should track the position of the last **yielded** event, not the last **read** event:

```rust
// Fixed implementation
if !rows.is_empty() {
    let mut items = Vec::with_capacity(rows.len());
    for row in rows.iter() {
        if let Ok(event) = row_to_event(row) {
            items.push(event);
        }
    }

    if let Some(first_event) = items.first() {
        // Only advance cursor to the position we're about to yield
        let yielded_nonce = first_event.meta.as_ref()
            .map(|m| m.global_nonce)
            .unwrap_or(cursor as u64) as i64;

        let event = first_event.clone();
        let remaining = if items.len() > 1 {
            items[1..].to_vec()
        } else {
            Vec::new()
        };

        let next_phase = if remaining.is_empty() {
            Phase::Live { cursor: yielded_nonce, interval }
        } else {
            Phase::Replay {
                items: remaining,
                idx: 0,
                cursor: yielded_nonce,  // Cursor reflects last yielded
            }
        };

        Some((Ok(SubscribeResponse { event: Some(event) }), next_state))
    }
}
```

### Key Invariants

1. **Cursor = last yielded event's position** (not last read)
2. **On stream interruption**: Client resumes from cursor, re-reading any events that were read but not yielded
3. **Idempotency**: Projections should be idempotent to handle potential duplicate delivery

## Consequences

### Positive

- **No lost events**: Events cannot be skipped due to premature cursor advancement
- **Correct semantics**: Aligns with established event sourcing patterns
- **Reliable projections**: Subscribers can trust they receive all events
- **Industry standard**: Matches how EventStoreDB and other event stores handle subscriptions

### Negative

- **Potential duplicate delivery**: If client saves position after processing but stream interrupts before next yield, the same event may be delivered again on reconnect
  - **Mitigation**: Projections should be idempotent (best practice regardless)

### Neutral

- No API changes required
- Backward compatible
- Existing clients work without modification

## Alternatives Considered

### Alternative 1: Batch Acknowledgment

Require explicit ACK from client before advancing cursor.

**Rejected because**:
- Adds complexity to API
- Not compatible with simple streaming model
- Would require protocol changes

### Alternative 2: Client-side Safety Buffer

Always request events from `position - N` and deduplicate client-side.

**Rejected because**:
- Workaround, not a fix
- Inefficient (re-processes events)
- Doesn't address root cause

### Alternative 3: Transactional Cursor Updates

Store cursor in same transaction as events, use 2PC.

**Rejected because**:
- Over-engineering for this use case
- Significant complexity increase
- Not necessary when cursor-after-yield is sufficient

## References

- [Persistent vs catch-up, EventStoreDB subscriptions in action](https://event-driven.io/en/persistent_vs_catch_up_eventstoredb_subscriptions_in_action/)
- [Let's talk about positions in event stores](https://event-driven.io/en/lets_talk_about_positions_in_event_stores/)
- ADR-012: ReadAll Global Stream
