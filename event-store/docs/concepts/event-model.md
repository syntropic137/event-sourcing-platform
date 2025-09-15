# Event Model and Envelope

This spec is the polyglot contract between clients and the Event Store.

- __Envelope fields__
  - eventType: string (dotted name)
  - schemaVersion: number (required)
  - eventId: string UUID (required; UUID v7 recommended)
  - contentType: string (e.g., `application/json`)
  - correlationId?: string
  - causationId?: string
  - actorId?: string
  - tenantId?: string
  - metadata?: `Record<string,string>`
  - payload: bytes

- __Client-provided on append__
  - aggregateId, aggregateType (identifies target aggregate)
  - aggregateNonce (proposed next sequence number)
  - expected: exact | NO_AGGREGATE | AGGREGATE_EXISTS (additional concurrency check)

- __Store-assigned on write__
  - globalNonce (global sequence, assigned atomically)
  - timestampUnixMs

> **Optimistic Concurrency Note**: Client proposes aggregateNonce, store validates it's correct (current_max + 1). This is true optimistic concurrency where client controls sequencing but store ensures correctness.

- __Append request (Option B)__
  - aggregateId, aggregateType
  - expected: exact | ANY | NO_AGGREGATE | AGGREGATE_EXISTS
  - events: ClientEvent[] (each has meta.aggregateNonce + payload)
  - Store validates: proposed aggregateNonce == current_max + 1

- __Append response__
  - nextAggregateNonce
  - nextGlobalNonce

- __Read/Subscribe__
  - Return fully materialized events with complete metadata.
