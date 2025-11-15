/**
 * Thin gRPC adapter that maps Event Sourcing envelopes to the Event Store TS SDK.
 */

import { EventSerializer, type EventEnvelope } from '../core/event';
import type { EventStoreClient as RepoEventStoreClient } from '../client/event-store-client';
import { EventStoreError } from '../core/errors';
import type { JsonObject, JsonValue } from '../types/common';

type EventStoreSdkModule = typeof import('@eventstore/sdk-ts');

const dynamicImport: <T>(specifier: string) => Promise<T> = (specifier) =>
  new Function('s', 'return import(s);')(specifier);

async function loadEventStoreClient(): Promise<EventStoreSdkModule['EventStoreClientTS']> {
  const mod = (await dynamicImport<EventStoreSdkModule>(
    '@eventstore/sdk-ts'
  )) as EventStoreSdkModule;
  return mod.EventStoreClientTS;
}

type EventStoreClient = import('@eventstore/sdk-ts').EventStoreClientTS;

type ReadStreamMetadata = {
  eventId: string;
  aggregateId: string;
  aggregateType?: string;
  aggregateNonce: number;
  eventType: string;
  eventVersion?: number;
  contentType?: string;
  contentSchema?: string;
  correlationId?: string;
  causationId?: string;
  actorId?: string;
  tenantId?: string;
  timestampUnixMs?: number;
  recordedTimeUnixMs?: number;
  payloadSha256?: Uint8Array | Buffer;
  headers?: Record<string, string>;
  globalNonce?: number;
};

export interface GrpcEventStoreConfig {
  /** gRPC address, e.g. localhost:50051 */
  serverAddress: string;
  /** Tenant identifier for multi-tenant stores */
  tenantId: string;
}

/**
 * Adapter implementing the repository EventStoreClient interface using the
 * Event Store gRPC TypeScript SDK.
 */
export class GrpcEventStoreAdapter implements RepoEventStoreClient {
  private readonly clientPromise: Promise<EventStoreClient>;
  private readonly tenantId: string;

  constructor(cfg: GrpcEventStoreConfig) {
    this.clientPromise = loadEventStoreClient().then((Ctor) => new Ctor(cfg.serverAddress));
    this.tenantId = cfg.tenantId;
  }

  async readEvents(streamName: string, fromVersion = 1): Promise<EventEnvelope[]> {
    const client = await this.clientPromise;
    const { aggregateId, aggregateType } = parseStreamName(streamName);

    const pageSize = 200;
    let next = fromVersion;
    const all: EventEnvelope[] = [];

    try {
      for (; ;) {
        const resp = await client.readStream({
          tenantId: this.tenantId,
          aggregateId,
          fromAggregateNonce: next,
          maxCount: pageSize,
          forward: true,
        });

        for (const e of resp.events) {
          if (!e.meta) continue;
          const meta = e.meta as ReadStreamMetadata;
          const payloadJson = safeJsonParse(e.payload);

          const recordedUnixMs = Number(
            meta.recordedTimeUnixMs ?? meta.timestampUnixMs ?? Date.now()
          );
          const timestampUnixMs = Number(meta.timestampUnixMs ?? recordedUnixMs);
          const recordedIso = new Date(recordedUnixMs).toISOString();
          const timestampIso = new Date(timestampUnixMs).toISOString();

          const metadataJson: JsonObject = {
            eventId: meta.eventId,
            timestamp: timestampIso,
            recordedTimestamp: recordedIso,
            aggregateNonce: Number(meta.aggregateNonce),
            aggregateId: meta.aggregateId,
            aggregateType: meta.aggregateType || aggregateType,
            tenantId: meta.tenantId || '',
            globalNonce: meta.globalNonce ?? null,
            contentType: meta.contentType || 'application/json',
            correlationId: meta.correlationId || '',
            causationId: meta.causationId || '',
            actorId: meta.actorId || '',
            headers: meta.headers ?? {},
            customMetadata: {},
          };

          if (meta.payloadSha256 && meta.payloadSha256.length > 0) {
            metadataJson.payloadHash = bytesToHex(meta.payloadSha256);
          }

          const eventJson: JsonObject = {
            eventType: meta.eventType,
            schemaVersion: meta.eventVersion ?? 0,
            data: payloadJson,
          };

          const envelopeJson: JsonObject = {
            event: eventJson,
            metadata: metadataJson,
          };

          all.push(EventSerializer.deserialize(envelopeJson));
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
    expectedAggregateNonce?: number
  ): Promise<void> {
    const { aggregateId, aggregateType } = parseStreamName(streamName);
    try {
      const aggType = (events[0]?.metadata.aggregateType as string) || aggregateType;
      const tenantId = resolveTenantId(events, this.tenantId);
      const nonce = expectedAggregateNonce ?? 0;

      const client = await this.clientPromise;
      await client.appendTyped({
        tenantId,
        aggregateId,
        aggregateType: aggType,
        expectedAggregateNonce: nonce,
        events: events.map((env) => ({
          meta: {
            aggregateNonce: Number(env.metadata.aggregateNonce),
            eventType: env.event.eventType,
            eventVersion: (env.event as { schemaVersion?: number }).schemaVersion ?? 0,
            eventId: env.metadata.eventId,
            contentType: env.metadata.contentType,
            correlationId: env.metadata.correlationId,
            causationId: env.metadata.causationId,
            actorId: env.metadata.actorId,
            tenantId: env.metadata.tenantId ?? tenantId,
            timestampUnixMs: parseTimestamp(env.metadata.timestamp),
            payloadSha256:
              env.metadata.payloadHash !== undefined
                ? hexToBytes(env.metadata.payloadHash)
                : undefined,
            headers: env.metadata.headers,
          },
          payload: Buffer.from(JSON.stringify(env.event.toJson())),
        })),
      });
    } catch (err) {
      const original = err as Error;
      // Surface gRPC status details during tests/troubleshooting
      if (process.env.NODE_ENV !== 'production') {
        console.error('appendEvents error', original);
      }
      throw new EventStoreError(
        `appendEvents failed for ${aggregateType}:${aggregateId}`,
        original
      );
    }
  }

  async streamExists(streamName: string): Promise<boolean> {
    const { aggregateId } = parseStreamName(streamName);
    try {
      const client = await this.clientPromise;
      const resp = await client.readStream({
        tenantId: this.tenantId,
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

  async connect(): Promise<void> {
    await this.clientPromise;
  }

  async disconnect(): Promise<void> {
    // No explicit disconnect required; channel lifecycle is managed by Node
  }
}

function safeJsonParse(buf: Uint8Array): JsonValue {
  try {
    return JSON.parse(Buffer.from(buf).toString('utf8')) as JsonValue;
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

function bytesToHex(bytes: Uint8Array | Buffer): string {
  return Array.from(bytes)
    .map((x) => x.toString(16).padStart(2, '0'))
    .join('');
}

function hexToBytes(hex: string): Uint8Array {
  const normalized = hex.length % 2 === 0 ? hex : `0${hex}`;
  const bytes = new Uint8Array(normalized.length / 2);
  for (let i = 0; i < bytes.length; i += 1) {
    bytes[i] = parseInt(normalized.slice(i * 2, i * 2 + 2), 16);
  }
  return bytes;
}

function resolveTenantId(events: EventEnvelope[], fallbackTenantId: string): string {
  const tenants = new Set<string>();
  for (const event of events) {
    if (event.metadata.tenantId) {
      tenants.add(event.metadata.tenantId);
    }
  }

  if (tenants.size > 1) {
    throw new EventStoreError('Append batch contains multiple tenant IDs');
  }

  const [tenantId] = tenants;
  return tenantId ?? fallbackTenantId;
}

function parseTimestamp(value?: string): number {
  if (!value) {
    return Date.now();
  }
  const parsed = Date.parse(value);
  return Number.isNaN(parsed) ? Date.now() : parsed;
}
