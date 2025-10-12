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
      async connect() {
        // no-op; underlying gRPC client is ready on construction
      },
      async disconnect() {
        // no-op; underlying gRPC client uses channel managed by Node
      },
    } satisfies EventStoreClient;
  }
}
