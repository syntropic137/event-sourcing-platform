/**
 * Repository pattern for loading and saving aggregates
 */

import { AggregateId } from '../types/common';
import { Aggregate } from './aggregate';
import { ConcurrencyConflictError, EventStoreError } from './errors';
import type { EventStoreClient } from '../client/event-store-client';

/** Repository interface for aggregate persistence */
export interface Repository<TAggregate extends Aggregate> {
  /** Load an aggregate by ID */
  load(aggregateId: AggregateId): Promise<TAggregate | null>;

  /** Save an aggregate */
  save(aggregate: TAggregate): Promise<void>;

  /** Check if an aggregate exists */
  exists(aggregateId: AggregateId): Promise<boolean>;
}

/** Repository implementation using the event store */
export class EventStoreRepository<TAggregate extends Aggregate> implements Repository<TAggregate> {
  constructor(
    private eventStoreClient: EventStoreClient,
    private aggregateFactory: () => TAggregate,
    private aggregateType: string
  ) {}

  /** Load an aggregate by ID */
  async load(aggregateId: AggregateId): Promise<TAggregate | null> {
    try {
      const streamName = this.getStreamName(aggregateId);

      // Check if stream exists
      const exists = await this.eventStoreClient.streamExists(streamName);
      if (!exists) {
        return null;
      }

      // Read events from the stream
      const events = await this.eventStoreClient.readEvents(streamName);

      if (events.length === 0) {
        return null;
      }

      // Create aggregate and load from events
      const aggregate = this.aggregateFactory();
      aggregate.rehydrate(events);

      return aggregate;
    } catch (error) {
      throw new EventStoreError(
        `Failed to load aggregate ${this.aggregateType}:${aggregateId}`,
        error instanceof Error ? error : undefined
      );
    }
  }

  /** Save an aggregate */
  async save(aggregate: TAggregate): Promise<void> {
    if (!aggregate.id) {
      throw new Error('Cannot save aggregate without an ID');
    }

    if (!aggregate.hasUncommittedEvents()) {
      return; // Nothing to save
    }

    try {
      const streamName = this.getStreamName(aggregate.id);
      const events = aggregate.getUncommittedEvents();

      // Calculate expected version (current version - number of new events)
      const expectedVersion = aggregate.version - events.length;

      // Append events to the stream
      await this.eventStoreClient.appendEvents(streamName, events, expectedVersion);

      // Mark events as committed
      aggregate.markEventsAsCommitted();
    } catch (error) {
      if (this.isConcurrencyError(error)) {
        const pendingEvents = aggregate.getUncommittedEvents().length;
        const expectedVersion = aggregate.version - pendingEvents;
        throw new ConcurrencyConflictError(expectedVersion, aggregate.version);
      }

      throw new EventStoreError(
        `Failed to save aggregate ${this.aggregateType}:${aggregate.id}`,
        error instanceof Error ? error : undefined
      );
    }
  }

  /** Check if an aggregate exists */
  async exists(aggregateId: AggregateId): Promise<boolean> {
    try {
      const streamName = this.getStreamName(aggregateId);
      return await this.eventStoreClient.streamExists(streamName);
    } catch (error) {
      throw new EventStoreError(
        `Failed to check existence of aggregate ${this.aggregateType}:${aggregateId}`,
        error instanceof Error ? error : undefined
      );
    }
  }

  /** Get the stream name for an aggregate */
  private getStreamName(aggregateId: AggregateId): string {
    return `${this.aggregateType}-${aggregateId}`;
  }

  /** Check if an error is a concurrency error */
  private isConcurrencyError(error: unknown): boolean {
    if (!error) {
      return false;
    }

    if (error instanceof ConcurrencyConflictError) {
      return true;
    }

    if (error instanceof Error) {
      const message = error.message.toLowerCase();
      return (
        message.includes('concurrency') ||
        message.includes('expected version') ||
        message.includes('precondition')
      );
    }

    if (typeof error === 'object' && error !== null) {
      const code = (error as { code?: unknown }).code;
      if (typeof code === 'string') {
        return code.toUpperCase() === 'FAILED_PRECONDITION' || code.toUpperCase() === 'ABORTED';
      }
    }

    return false;
  }
}

/** Repository factory for creating repositories */
export class RepositoryFactory {
  constructor(private eventStoreClient: EventStoreClient) {}

  /** Create a repository for a specific aggregate type */
  createRepository<TAggregate extends Aggregate>(
    aggregateFactory: () => TAggregate,
    aggregateType: string
  ): Repository<TAggregate> {
    return new EventStoreRepository(this.eventStoreClient, aggregateFactory, aggregateType);
  }
}

/** Repository registry for managing multiple repositories */
type AnyAggregate = Aggregate;
type AnyRepository = Repository<AnyAggregate>;

export class RepositoryRegistry {
  private repositories = new Map<string, AnyRepository>();

  /** Register a repository */
  register<TAggregate extends Aggregate>(
    aggregateType: string,
    repository: Repository<TAggregate>
  ): void {
    this.repositories.set(aggregateType, repository as AnyRepository);
  }

  /** Get a repository by aggregate type */
  get<TAggregate extends Aggregate>(aggregateType: string): Repository<TAggregate> | null {
    const repository = this.repositories.get(aggregateType) as Repository<TAggregate> | undefined;
    return repository ?? null;
  }

  /** Get a repository by aggregate type (throws if not found) */
  getRequired<TAggregate extends Aggregate>(aggregateType: string): Repository<TAggregate> {
    const repository = this.get<TAggregate>(aggregateType);
    if (!repository) {
      throw new Error(`No repository registered for aggregate type: ${aggregateType}`);
    }
    return repository;
  }
}
