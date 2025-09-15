# SDK Design (TypeScript)

- __High-level API__
  - appendEvents({ aggregateId, aggregateType, expected, events }) -> { nextAggregateNonce, lastGlobalNonce }
  - readStream(aggregateId)
  - subscribe({ fromGlobalNonce })

- __Decorators__
  - `@Event({ type: 'com.myco.order.OrderCreated', revision: 2 })`
  - Helper `toClientEvent(payload, opts)` reads decorator metadata, adds eventId (UUID v7), contentType, message context (correlation/causation), metadata.

- __Message context__
  - Async-local storage to propagate correlationId/causationId.

- __Type-safety__
  - `appendEvents()` enforces presence of meta.eventType and meta.schemaVersion at compile-time.
  - Store-managed fields are not settable by the client.

- __Examples__
  - See `src/examples/typed-basic.ts` using decorators and `appendEvents()`.
