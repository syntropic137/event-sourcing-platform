/**
 * Thin gRPC adapter that maps Event Sourcing envelopes to the Event Store TS SDK.
 */

import { EventSerializer, type EventEnvelope } from '../core/event';
import type { EventStoreClient as RepoEventStoreClient } from '../core/repository';
import { EventStoreError } from '../core/errors';
import { EventStoreClientTS, Expected } from '@eventstore/sdk-ts';
import type { JsonValue } from '../types/common';

export interface GrpcEventStoreConfig {
  /** gRPC address, e.g. localhost:50051 */
  serverAddress: string;
}

/**
 * Adapter implementing the repository EventStoreClient interface using the
 * Event Store gRPC TypeScript SDK.
 */
export class GrpcEventStoreAdapter implements RepoEventStoreClient {
  private readonly client: EventStoreClientTS;

  constructor(cfg: GrpcEventStoreConfig) {
    this.client = new EventStoreClientTS(cfg.serverAddress);
  }

  async readEvents(streamName: string, fromVersion = 1): Promise<EventEnvelope[]> {
    const { aggregateId, aggregateType } = parseStreamName(streamName);

    const pageSize = 200;
    let next = fromVersion;
    const all: EventEnvelope[] = [];

    try {
      for (;;) {
        const resp = await this.client.readStream({
          aggregateId,
          fromAggregateNonce: next,
          maxCount: pageSize,
          forward: true,
        });

        for (const e of resp.events) {
          if (!e.meta) continue;
          const meta = e.meta;
          const payloadJson = safeJsonParse(e.payload);
          const json = {
            event: {
              eventType: meta.eventType,
              schemaVersion: meta.schemaVersion ?? 0,
              data: payloadJson,
            },
            metadata: {
              eventId: meta.eventId,
              timestamp: new Date(Number(meta.timestampUnixMs)).toISOString(),
              aggregateVersion: Number(meta.aggregateNonce),
              aggregateId: meta.aggregateId,
              aggregateType: meta.aggregateType || aggregateType,
              metadata: {
                contentType: meta.contentType,
                headers: meta.headers || {},
                correlationId: meta.correlationId,
                causationId: meta.causationId,
                actorId: meta.actorId,
                tenantId: meta.tenantId,
                globalNonce: meta.globalNonce,
              },
            },
          } as const;
          all.push(EventSerializer.deserialize(json));
        }

        if (resp.isEnd) break;
        next = resp.nextFromAggregateNonce;
      }
      return all;
    } catch (err) {
      throw new EventStoreError(
        `readEvents failed for ${aggregateType}:${aggregateId}`,
        err as Error
      );
    }
  }

  async appendEvents(
    streamName: string,
    events: EventEnvelope[],
    expectedVersion?: number
  ): Promise<void> {
    const { aggregateId, aggregateType } = parseStreamName(streamName);
    try {
      const aggType = (events[0]?.metadata.aggregateType as string) || aggregateType;
      const firstWrite = expectedVersion === 0;
      await this.client.appendTyped({
        aggregateId,
        aggregateType: aggType,
        exact: firstWrite ? undefined : expectedVersion,
        expectedAny: firstWrite ? Expected.NO_AGGREGATE : undefined,
        events: events.map((env) => ({
          meta: {
            aggregateNonce: Number(env.metadata.aggregateVersion),
            eventType: env.event.eventType,
            eventId: env.metadata.eventId,
            contentType: 'application/json',
            schemaVersion: (env.event as any).schemaVersion ?? 0,
          },
          payload: Buffer.from(JSON.stringify(env.event.toJson())),
        })),
      });
    } catch (err) {
      throw new EventStoreError(
        `appendEvents failed for ${aggregateType}:${aggregateId}`,
        err as Error
      );
    }
  }

  async streamExists(streamName: string): Promise<boolean> {
    const { aggregateId } = parseStreamName(streamName);
    try {
      const resp = await this.client.readStream({
        aggregateId,
        fromAggregateNonce: 1,
        maxCount: 1,
        forward: true,
      });
      return resp.events.length > 0;
    } catch {
      return false;
    }
  }
}

function safeJsonParse(buf: Buffer): JsonValue {
  try {
    return JSON.parse(buf.toString('utf8')) as JsonValue;
  } catch {
    // Fall back to an empty object which is a valid JsonValue (JsonObject)
    return {};
  }
}

function parseStreamName(stream: string): { aggregateType: string; aggregateId: string } {
  const idx = stream.indexOf('-');
  if (idx === -1) return { aggregateType: '', aggregateId: stream };
  const aggregateType = stream.slice(0, idx);
  const aggregateId = stream.slice(idx + 1);
  return { aggregateType, aggregateId };
}
