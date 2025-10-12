# Event Store vs Event Bus

This project provides a polyglot Event Store and a TypeScript SDK. It intentionally separates storage concerns from publishing/handling concerns.

- __Event Store__
  - Responsibilities: durable append, sequencing (aggregateNonce), global tracking (globalNonce), reads, subscriptions, idempotency and concurrency checks.
  - Canonical identifiers: aggregateId, aggregateType.
  - Authoritative fields assigned by the store: aggregateNonce, globalNonce, timestamp.

- __Event Bus__
  - Responsibilities: delivering events to handlers (in-process or distributed), backpressure, retry policies, DLQ, routing.
  - Can be implemented by Axon Server, Kafka, AMQP, or an embedded in-process bus.
  - When using Axon, the bus and store can be combined (AxonServerEventStore) or separated (EmbeddedEventStore + external bus).

- __Decorators (SDK ergonomics)__
  - In TS, decorators and helpers shape your event payloads into a canonical envelope (eventType + schemaVersion + metadata + payload) for the Event Store.
  - They also provide handler ergonomics locally (subscribe/dispatch), but they are not the Event Store themselves.
  - Message context utilities can stamp correlationId/causationId automatically.

- __Interop strategy__
  - On the wire: use the envelope spec from `event-model.md`.
  - For Axon interop: eventType uses dotted names; schemaVersion maps to @Revision; metadata maps to Axon MetaData.
  - For non-Java stacks: follow the same envelope; implement decorators/helpers per language.

- __Typical flows__
  1) Command handled in an aggregate → emits one or more events.
  2) SDK constructs ClientEvent(s) from payloads and metadata → append to Event Store with the current stream head as `expectedAggregateNonce`.
  3) Projections/processors subscribe from a globalNonce (tracking token) or per-aggregate stream to react/build read models.
  4) Optionally, events are also published on an external bus for cross-bounded-context distribution.
