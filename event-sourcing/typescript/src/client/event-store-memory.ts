/**
 * In-memory event store client implementation.
 * Suitable for local development and unit tests (not a mock; behaves like a store).
 */

import type { EventEnvelope } from '../core/event';
import { EventStoreError } from '../core/errors';
import type { EventStoreClient } from './event-store-client';

export class MemoryEventStoreClient implements EventStoreClient {
  private readonly streams = new Map<string, EventEnvelope[]>();

  async readEvents(streamName: string, fromVersion?: number): Promise<EventEnvelope[]> {
    const events = this.streams.get(streamName) ?? [];
    const startIndex = fromVersion ? Math.max(0, fromVersion - 1) : 0;
    return events.slice(startIndex);
  }

  async appendEvents(
    streamName: string,
    events: EventEnvelope[],
    expectedVersion?: number
  ): Promise<void> {
    const existing = this.streams.get(streamName) ?? [];
    if (expectedVersion !== undefined && existing.length !== expectedVersion) {
      throw new EventStoreError(
        `Concurrency conflict: expected ${expectedVersion}, got ${existing.length}`
      );
    }
    this.streams.set(streamName, existing.concat(events));
  }

  async streamExists(streamName: string): Promise<boolean> {
    const arr = this.streams.get(streamName);
    return !!arr && arr.length > 0;
  }

  async connect(): Promise<void> {
    // no-op
  }

  async disconnect(): Promise<void> {
    // no-op
  }
}
