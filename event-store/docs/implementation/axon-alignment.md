# Axon Alignment

- __Terminology__
  - eventType ↔ Axon payload class name (FQCN-style dotted name)
  - schemaVersion ↔ Axon @Revision
  - metadata ↔ Axon MetaData
  - streamVersion ↔ sequence number (Axon term)
  - globalNonce ↔ tracking token

- __Behavior__
  - Server assigns aggregateNonce and globalNonce.
  - Clients use Expected Version for optimistic concurrency.
  - Correlation/Causation IDs propagated via message context.

  Note: We standardize on the term `streamVersion` in our API/docs. This is exactly Axon’s per-aggregate "sequence number".

- __Wire compatibility__
  - Use dotted names for eventType to align with Axon conventions.
  - JSON/Protobuf payloads supported; contentType recorded in metadata.

- __Evolution__
  - Upcasters can transform older (schemaVersion < current) events on read.
  - Optional registry for schema discovery via dataSchemaUri.
