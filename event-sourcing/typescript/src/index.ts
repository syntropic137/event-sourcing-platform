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
export type { EventStoreClient, EventStoreClientConfig } from './client/event-store-client';
export { EventStoreClientFactory } from './client/event-store-client';

// gRPC adapter (thin wrapper around event-store TS SDK)
export { GrpcEventStoreAdapter } from './integrations/grpc-event-store';

// In-memory store for local dev and tests
export { MemoryEventStoreClient } from './client/event-store-memory';

// Utilities and helpers
export * from './utils/decorators';
// Note: AggregateRoot decorator from utils/metadata is NOT exported here
// to avoid collision with AggregateRoot base class from core/aggregate.
// Use @Aggregate decorator instead (exported below).

// Types
export * from './types/common';

// Re-export commonly used types for convenience
export type {
  DomainEvent,
  EventEnvelope,
  EventMetadata,
  Repository,
  Projection,
  Query,
  QueryHandler,
  QueryResult,
} from './core';

// Re-export decorators and classes with convenient names
export {
  AggregateDecorator as Aggregate,
  EventSourcingHandler,
  AggregateRoot,
  AutoDispatchAggregate,
} from './core/aggregate';

export { CommandHandler } from './core/command';

// Re-export commonly used classes
export { EventSerializer, BaseDomainEvent } from './core/event';
export { RepositoryFactory } from './core/repository';
