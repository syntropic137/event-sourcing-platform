# ADR 001: Client-Proposed Optimistic Concurrency

## Status
Accepted

## Context
We needed to choose between two optimistic concurrency models for our event store:

**Option A: Store-Assigned Sequences**
- Store assigns all sequence numbers (aggregate_nonce, global_nonce)
- Client provides expected version for validation
- Store has complete control over sequencing
- Simple for clients, no guessing required

**Option B: Client-Proposed Sequences**
- Client proposes aggregate_nonce based on current state
- Store validates proposed nonce == current_max + 1
- Client controls sequencing, store ensures correctness
- Requires clients to know current sequence state

## Decision
We chose **Option B: Client-Proposed Sequences** for true optimistic concurrency.

## Rationale

### Why Client-Proposed is Better

1. **True Optimistic Concurrency**
   - Client controls event ordering and sequencing
   - Store validates correctness without conflicts
   - Matches blockchain transaction nonce model

2. **Race Condition Prevention**
   - Only one client can propose the correct next nonce
   - Multiple clients proposing same nonce get ConcurrencyError
   - No possibility of conflicting sequence assignments

3. **Client Autonomy**
   - Client decides event ordering within their aggregate
   - Store ensures global consistency
   - Better for complex business logic scenarios

### Concurrency Flow
```
Initial state: aggregate_nonce = 4

Client A: Read current = 4, propose nonce = 5 ✅
Client B: Read current = 4, propose nonce = 5 ❌ (ConcurrencyError)

Client B: Retry with nonce = 6 → Store validates 6 == 5+1 ✅
```

## Consequences

### Positive
- ✅ True optimistic concurrency model
- ✅ No race conditions in sequence assignment
- ✅ Client has control over event ordering
- ✅ Matches industry best practices (EventStoreDB, Axon)

### Negative
- ❌ Client must know current sequence state
- ❌ More complex client implementation
- ❌ Requires client to handle ConcurrencyError retries

### Implementation Details
- Client reads current aggregate_nonce before proposing
- Client proposes `current_nonce + 1` in EventMetadata
- Store validates proposal == `current_max + 1`
- Store assigns global_nonce atomically
- ConcurrencyError returned if validation fails

## Alternatives Considered

### Store-Assigned Sequences
- Rejected because it removes client control over sequencing
- Doesn't provide true optimistic concurrency
- Store becomes bottleneck for sequence decisions

## References
- EventStoreDB optimistic concurrency model
- Axon Framework sequence number handling
- Ethereum transaction nonces
