# Concurrency and Consistency

- __Optimistic Concurrency__ via Expected Version:
  - `exact(n)`: must match current aggregateNonce = n
  - `NO_AGGREGATE`: aggregate must not exist
  - `AGGREGATE_EXISTS`: aggregate must exist
  - `ANY`: no concurrency check

- __Client-Proposed, Store-Validated Nonces (True Optimistic Concurrency)__
  - **Client Responsibility**: Client proposes aggregateNonce based on current state
  - **Store Validation**: Store validates proposed nonce == current_max + 1
  - **Concurrency Control**: If validation fails, store rejects with ConcurrencyError
  - **Atomic Assignment**: Store assigns globalNonce atomically (still store-controlled)

- __Concurrency Control Flow Example__
  ```
  Initial state: Order-123 has aggregateNonce = 4

  Client A: Read aggregateNonce → 4, proposes nonce = 5
  Client B: Read aggregateNonce → 4, proposes nonce = 5

  Client A: Append with proposed_nonce = 5 → Store validates 5 == 4+1 ✅ → accepts
  Client B: Append with proposed_nonce = 5 → Store validates 5 == 5+1 ❌ → ConcurrencyError

  Client B: Retry with proposed_nonce = 6 → Store validates 6 == 5+1 ✅ → accepts
  ```

- __Why This Works__
  - **True Optimistic Concurrency**: Client proposes, store validates (like blockchain)
  - **Client Controls Sequencing**: Client decides event ordering
  - **Store Ensures Correctness**: Validates proposed nonce is exactly current_max + 1
  - **No Race Conditions**: Only one client can propose the correct next nonce

- __Database Enforcement__
  - UNIQUE(aggregate_id, aggregate_nonce)
  - UNIQUE(event_id)
  - global_nonce as identity/bigserial
  - Indexes on global_nonce and (aggregate_type, aggregate_id, aggregate_nonce)

- __Idempotency__
  - eventId uniqueness ensures safe retries without duplicates
