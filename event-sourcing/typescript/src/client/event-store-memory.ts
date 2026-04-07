/**
 * In-memory event store client implementation.
 * Suitable for unit tests ONLY (not a mock; behaves like a store).
 *
 * WARNING: This client can ONLY be used in test environments.
 * It will throw an error if NODE_ENV is not 'test'.
 * For development and production, use GrpcEventStoreAdapter.
 */

import type { EventEnvelope } from '../core/event';
import { EventStoreError } from '../core/errors';
import type { EventStoreClient, ReadAllResult } from './event-store-client';

/**
 * Assert test environment - REQUIRED for all mocks.
 * Prevents mock objects from being used in development or production.
 */
function assertTestEnvironment(): void {
  const nodeEnv = process.env.NODE_ENV?.toLowerCase() ?? '';
  if (nodeEnv !== 'test') {
    throw new Error(
      `MemoryEventStoreClient can only be used in test environment. ` +
        `Current NODE_ENV: '${nodeEnv}'. ` +
        `Set NODE_ENV=test for unit tests, or use GrpcEventStoreAdapter for development/production.`
    );
  }
}

export class MemoryEventStoreClient implements EventStoreClient {
  private readonly streams = new Map<string, EventEnvelope[]>();
  private globalNonceCounter = 0;

  constructor() {
    // CRITICAL: Validate test environment before allowing usage
    assertTestEnvironment();
  }

  async readEvents(streamName: string, fromVersion?: number): Promise<EventEnvelope[]> {
    const events = this.streams.get(streamName) ?? [];
    const startIndex = fromVersion ? Math.max(0, fromVersion - 1) : 0;
    return events.slice(startIndex);
  }

  async appendEvents(
    streamName: string,
    events: EventEnvelope[],
    expectedAggregateNonce?: number
  ): Promise<void> {
    const existing = this.streams.get(streamName) ?? [];
    if (expectedAggregateNonce !== undefined && existing.length !== expectedAggregateNonce) {
      throw new EventStoreError(
        `Concurrency conflict: expected aggregate nonce ${expectedAggregateNonce}, got ${existing.length}`
      );
    }

    // Assign global nonces to events
    const eventsWithGlobalNonce = events.map((event) => {
      this.globalNonceCounter++;
      return {
        ...event,
        metadata: {
          ...event.metadata,
          globalNonce: this.globalNonceCounter,
        },
      };
    });

    this.streams.set(streamName, existing.concat(eventsWithGlobalNonce));
  }

  async streamExists(streamName: string): Promise<boolean> {
    const arr = this.streams.get(streamName);
    return !!arr && arr.length > 0;
  }

  async readAll(fromGlobalNonce = 0, maxCount = 100, forward = true): Promise<ReadAllResult> {
    // Collect all events from all streams
    const allEvents: EventEnvelope[] = [];
    for (const streamEvents of this.streams.values()) {
      allEvents.push(...streamEvents);
    }

    // Filter by global nonce
    const filtered = allEvents.filter((event) => {
      const globalNonce = event.metadata.globalNonce ?? 0;
      return forward ? globalNonce >= fromGlobalNonce : globalNonce <= fromGlobalNonce;
    });

    // Sort by global nonce
    filtered.sort((a, b) => {
      const nonceA = a.metadata.globalNonce ?? 0;
      const nonceB = b.metadata.globalNonce ?? 0;
      return forward ? nonceA - nonceB : nonceB - nonceA;
    });

    // Apply limit
    const page = filtered.slice(0, maxCount);

    // Determine if we've reached the end
    const isEnd = page.length < maxCount;

    // Calculate next position
    let nextFromGlobalNonce = fromGlobalNonce;
    if (page.length > 0) {
      const lastEvent = forward ? page[page.length - 1] : page[0];
      const lastNonce = lastEvent.metadata.globalNonce ?? 0;
      nextFromGlobalNonce = forward ? lastNonce + 1 : Math.max(0, lastNonce - 1);
    }

    return {
      events: page,
      isEnd,
      nextFromGlobalNonce,
    };
  }

  async connect(): Promise<void> {
    // no-op
  }

  async disconnect(): Promise<void> {
    // no-op
  }

  /** Clear all streams (useful for test cleanup) */
  clear(): void {
    this.streams.clear();
    this.globalNonceCounter = 0;
  }
}
