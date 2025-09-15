/**
 * Event Sourcing TypeScript SDK
 *
 * This SDK provides high-level abstractions for implementing event sourcing patterns
 * in TypeScript applications. It builds on top of the event-store gRPC API to provide
 * developer-friendly APIs for aggregates, commands, events, and repositories.
 */

// Core abstractions
export * from './core/aggregate';
export * from './core/command';
export * from './core/event';
export * from './core/repository';
export * from './core/query';
export * from './core/errors';

// Event store client integration
export {
  EventStoreClient,
  EventStoreClientConfig,
  EventStoreClientFactory,
} from './client/event-store-client';

// gRPC adapter (thin wrapper around event-store TS SDK)
export { GrpcEventStoreAdapter } from './integrations/grpc-event-store';

// In-memory store for local dev and tests
export { MemoryEventStoreClient } from './client/event-store-memory';

// Utilities and helpers
export * from './utils/decorators';
export * from './utils/metadata';

// Types
export * from './types/common';

// Re-export commonly used types for convenience
export type {
  DomainEvent,
  EventEnvelope,
  EventMetadata,
  CommandHandler,
  Aggregate,
  Repository,
  Projection,
  Query,
  QueryHandler,
  QueryResult,
} from './core';
