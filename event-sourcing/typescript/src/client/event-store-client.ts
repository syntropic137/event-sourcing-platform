/**
 * Event store client integration
 */

import { EventEnvelope } from '../core/event';
import { BaseConfig } from '../types/common';
import { GrpcEventStoreAdapter } from '../integrations/grpc-event-store';
import { MemoryEventStoreClient } from './event-store-memory';

/** Configuration for the event store client */
export interface EventStoreClientConfig extends BaseConfig {
  /** gRPC server address */
  serverAddress: string;

  /** Tenant identifier to scope requests */
  tenantId?: string;

  /** Connection timeout in milliseconds */
  timeoutMs?: number;
}

/** Result from readAll operation */
export interface ReadAllResult {
  /** Events in requested order */
  events: EventEnvelope[];
  /** True if no more events after this batch */
  isEnd: boolean;
  /** Position for next page (if !isEnd) */
  nextFromGlobalNonce: number;
}

/** Event store client interface */
export interface EventStoreClient {
  /** Read events from a stream */
  readEvents(streamName: string, fromVersion?: number): Promise<EventEnvelope[]>;

  /** Append events to a stream */
  appendEvents(
    streamName: string,
    events: EventEnvelope[],
    expectedAggregateNonce?: number
  ): Promise<void>;

  /** Check if a stream exists */
  streamExists(streamName: string): Promise<boolean>;

  /**
   * Read all events from a global position (for projections/catch-up).
   * @param fromGlobalNonce - Global nonce to read from (inclusive)
   * @param maxCount - Maximum number of events to return per page
   * @param forward - Direction (true = ascending order)
   * @returns Events, isEnd flag, and next position
   */
  readAll(fromGlobalNonce?: number, maxCount?: number, forward?: boolean): Promise<ReadAllResult>;

  /** Connect to the event store */
  connect(): Promise<void>;

  /** Disconnect from the event store */
  disconnect(): Promise<void>;
}

/** Factory for creating event store clients */
export class EventStoreClientFactory {
  /** Create an in-memory event store client (useful for local dev and tests) */
  static createMemoryClient(): EventStoreClient {
    return new MemoryEventStoreClient();
  }

  /** Create a gRPC-backed event store client using the event-store TS SDK */
  static createGrpcClient(config: EventStoreClientConfig): EventStoreClient {
    const adapter = new GrpcEventStoreAdapter({
      serverAddress: config.serverAddress,
      tenantId: config.tenantId ?? 'default',
    });
    // Provide a simple wrapper with connect/disconnect no-ops to match interface
    return {
      async readEvents(streamName: string, fromVersion?: number) {
        return adapter.readEvents(streamName, fromVersion);
      },
      async appendEvents(
        streamName: string,
        events: EventEnvelope[],
        expectedAggregateNonce?: number
      ) {
        return adapter.appendEvents(streamName, events, expectedAggregateNonce);
      },
      async streamExists(streamName: string) {
        return adapter.streamExists(streamName);
      },
      async readAll(fromGlobalNonce?: number, maxCount?: number, forward?: boolean) {
        return adapter.readAll(fromGlobalNonce, maxCount, forward);
      },
      async connect() {
        // no-op; underlying gRPC client is ready on construction
      },
      async disconnect() {
        // no-op; underlying gRPC client uses channel managed by Node
      },
    } satisfies EventStoreClient;
  }
}
