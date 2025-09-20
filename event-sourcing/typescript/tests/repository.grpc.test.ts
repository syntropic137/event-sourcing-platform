import { spawn, ChildProcess } from 'child_process';
import path from 'path';
import { randomUUID } from 'crypto';
import * as grpc from '@grpc/grpc-js';
import * as protoLoader from '@grpc/proto-loader';

import {
  EventSerializer,
  RepositoryFactory,
  type EventEnvelope,
  type EventStoreClient,
} from '../src';
import {
  OrderAggregate,
  OrderCancelled,
  OrderStatus,
  OrderSubmitted,
} from './helpers/order-aggregate';

jest.setTimeout(30000);

const EVENT_STORE_ADDR = '127.0.0.1:50062';
const TENANT_ID = 'tenant-integration';
const cargoArgs = ['run', '--quiet', '--package', 'eventstore-bin'];

const repoRoot = path.resolve(__dirname, '..', '..', '..');
const eventStoreDir = path.resolve(repoRoot, 'event-store');

let serverProcess: ChildProcess | null = null;

type NoArgEventCtor<T> = new () => T;

EventSerializer.registerEvent(
  'OrderSubmitted',
  OrderSubmitted as unknown as NoArgEventCtor<OrderSubmitted>
);
EventSerializer.registerEvent(
  'OrderCancelled',
  OrderCancelled as unknown as NoArgEventCtor<OrderCancelled>
);

const PROTO_PATH = path.resolve(
  repoRoot,
  'event-store',
  'eventstore-proto',
  'proto',
  'eventstore',
  'v1',
  'eventstore.proto'
);

const packageDefinition = protoLoader.loadSync(PROTO_PATH, {
  longs: String,
  enums: String,
  defaults: true,
  oneofs: true,
});

const proto = grpc.loadPackageDefinition(packageDefinition) as unknown as {
  eventstore: {
    v1: {
      EventStore: new (
        address: string,
        credentials: grpc.ChannelCredentials
      ) => GrpcEventStoreClient;
    };
  };
};

type AppendRequest = {
  tenantId: string;
  aggregateId: string;
  aggregateType: string;
  expectedAggregateNonce: number;
  idempotencyKey: string;
  events: Array<{
    meta?: Record<string, unknown>;
    payload: Buffer;
  }>;
};

type ReadStreamRequest = {
  tenantId: string;
  aggregateId: string;
  fromAggregateNonce: number;
  maxCount: number;
  forward: boolean;
};

type ReadStreamResponse = {
  events: Array<{
    meta?: Record<string, unknown>;
    payload: Buffer;
  }>;
  isEnd: boolean;
  nextFromAggregateNonce: number;
};

type AppendResponse = {
  lastAggregateNonce: number;
  lastGlobalNonce: number;
};

type GrpcEventStoreClient = grpc.Client & {
  append(
    request: AppendRequest,
    callback: (error: grpc.ServiceError | null, response: AppendResponse) => void
  ): void;
  readStream(
    request: ReadStreamRequest,
    callback: (error: grpc.ServiceError | null, response: ReadStreamResponse) => void
  ): void;
};

class NativeGrpcClient implements EventStoreClient {
  private readonly client: GrpcEventStoreClient;

  constructor(
    private readonly address: string,
    private readonly tenantId: string
  ) {
    const ctor = proto.eventstore.v1.EventStore;
    this.client = new ctor(address, grpc.credentials.createInsecure());
  }

  async readEvents(streamName: string, fromVersion: number = 1): Promise<EventEnvelope[]> {
    const { aggregateId } = parseStreamName(streamName);
    const request: ReadStreamRequest = {
      tenantId: this.tenantId,
      aggregateId,
      fromAggregateNonce: fromVersion,
      maxCount: 100,
      forward: true,
    };

    const response = await new Promise<ReadStreamResponse>((resolve, reject) => {
      this.client.readStream(request, (err, resp) => {
        if (err) {
          reject(err);
          return;
        }
        resolve(resp);
      });
    });

    return response.events.map(convertWireEventToEnvelope);
  }

  async appendEvents(
    streamName: string,
    events: EventEnvelope[],
    expectedVersion?: number
  ): Promise<void> {
    const { aggregateId, aggregateType } = parseStreamName(streamName);
    const request: AppendRequest = {
      tenantId: this.tenantId,
      aggregateId,
      aggregateType,
      expectedAggregateNonce: expectedVersion ?? 0,
      idempotencyKey: '',
      events: events.map((envelope) => convertEnvelopeToWireEvent(envelope, this.tenantId)),
    };

    await new Promise<void>((resolve, reject) => {
      this.client.append(request, (err, response) => {
        if (err) {
          reject(err);
          return;
        }
        if (!response) {
          reject(new Error('Missing AppendResponse'));
          return;
        }
        resolve();
      });
    });
  }

  async streamExists(streamName: string): Promise<boolean> {
    try {
      const events = await this.readEvents(streamName, 1);
      return events.length > 0;
    } catch {
      return false;
    }
  }

  async connect(): Promise<void> {
    // Client ready on construction
  }

  async disconnect(): Promise<void> {
    this.client.close();
  }
}

function convertEnvelopeToWireEvent(envelope: EventEnvelope, fallbackTenantId: string) {
  const metadata = envelope.metadata;
  return {
    meta: {
      eventId: metadata.eventId,
      aggregateId: metadata.aggregateId,
      aggregateType: metadata.aggregateType,
      aggregateNonce: metadata.aggregateVersion,
      eventType: envelope.event.eventType,
      eventVersion: (envelope.event as { schemaVersion?: number }).schemaVersion ?? 0,
      contentType: metadata.contentType,
      contentSchema: metadata.customMetadata?.['contentSchema'] ?? '',
      correlationId: metadata.correlationId ?? '',
      causationId: metadata.causationId ?? '',
      actorId: metadata.actorId ?? '',
      tenantId: metadata.tenantId ?? fallbackTenantId,
      timestampUnixMs: parseTimestamp(metadata.timestamp),
      recordedTimeUnixMs: parseTimestamp(metadata.recordedTimestamp),
      payloadSha256: metadata.payloadHash
        ? Buffer.from(metadata.payloadHash, 'hex')
        : new Uint8Array(),
      headers: metadata.headers ?? {},
      globalNonce: metadata.globalPosition ?? 0,
    },
    payload: Buffer.from(JSON.stringify(envelope.event.toJson())),
  };
}

function convertWireEventToEnvelope(event: {
  meta?: Record<string, unknown>;
  payload: Buffer;
}): EventEnvelope {
  const meta = event.meta ?? {};
  const metadataJson = {
    eventId: String(meta.eventId ?? ''),
    timestamp: new Date(Number(meta.timestampUnixMs ?? Date.now())).toISOString(),
    recordedTimestamp: new Date(Number(meta.recordedTimeUnixMs ?? Date.now())).toISOString(),
    aggregateVersion: Number(meta.aggregateNonce ?? 0),
    aggregateId: String(meta.aggregateId ?? ''),
    aggregateType: String(meta.aggregateType ?? ''),
    tenantId: meta.tenantId ? String(meta.tenantId) : undefined,
    globalPosition: meta.globalNonce === undefined ? undefined : Number(meta.globalNonce),
    contentType: String(meta.contentType ?? 'application/json'),
    correlationId: meta.correlationId ? String(meta.correlationId) : undefined,
    causationId: meta.causationId ? String(meta.causationId) : undefined,
    actorId: meta.actorId ? String(meta.actorId) : undefined,
    headers: (meta.headers as Record<string, string> | undefined) ?? {},
    payloadHash: undefined as string | undefined,
    customMetadata: {} as Record<string, unknown>,
  };

  const payloadJson = safeJsonParse(event.payload);

  return EventSerializer.deserialize({
    event: {
      eventType: String(meta.eventType ?? ''),
      schemaVersion: Number(meta.eventVersion ?? 0),
      data: payloadJson,
    },
    metadata: metadataJson,
  });
}

function parseTimestamp(value: string): number {
  const parsed = Date.parse(value);
  return Number.isNaN(parsed) ? Date.now() : parsed;
}

function safeJsonParse(buffer: Buffer): Record<string, unknown> {
  try {
    return JSON.parse(buffer.toString('utf8'));
  } catch {
    return {};
  }
}

function parseStreamName(stream: string): { aggregateId: string; aggregateType: string } {
  const idx = stream.indexOf('-');
  if (idx === -1) {
    return { aggregateId: stream, aggregateType: '' };
  }
  return { aggregateType: stream.slice(0, idx), aggregateId: stream.slice(idx + 1) };
}

async function wait(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitForServer(): Promise<void> {
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    const ctor = proto.eventstore.v1.EventStore;
    const client = new ctor(EVENT_STORE_ADDR, grpc.credentials.createInsecure());
    try {
      await new Promise<void>((resolve, reject) => {
        client.waitForReady(new Date(Date.now() + 1_000), (err) => {
          if (err) {
            reject(err);
            return;
          }
          resolve();
        });
      });
      client.close();
      return;
    } catch {
      client.close();
      await wait(300);
    }
  }
  throw new Error('Timed out waiting for Event Store gRPC endpoint');
}

beforeAll(async () => {
  serverProcess = spawn('cargo', cargoArgs, {
    cwd: eventStoreDir,
    env: {
      ...process.env,
      BIND_ADDR: EVENT_STORE_ADDR,
      BACKEND: 'memory',
      RUST_LOG: process.env.RUST_LOG ?? 'warn',
    },
    stdio: ['ignore', 'inherit', 'inherit'],
  });

  await waitForServer();
}, 30_000);

afterAll(async () => {
  if (serverProcess) {
    const current = serverProcess;
    serverProcess = null;
    current.kill();
    await new Promise((resolve) => current.once('exit', resolve));
  }
});

describe('Repository with gRPC Event Store', () => {
  it('persists and rehydrates an aggregate via gRPC', async () => {
    const client = new NativeGrpcClient(EVENT_STORE_ADDR, TENANT_ID);
    await client.connect();

    const repository = new RepositoryFactory(client).createRepository(
      () => new OrderAggregate(),
      'Order'
    );

    const orderId = `order-${randomUUID()}`;
    const aggregate = new OrderAggregate();
    aggregate.submit(orderId, 'customer-1');

    await repository.save(aggregate);

    const loaded = await repository.load(orderId);
    expect(loaded).not.toBeNull();
    expect(loaded?.getStatus()).toBe(OrderStatus.Submitted);

    loaded?.cancel('cancelled');
    await repository.save(loaded!);

    const reloaded = await repository.load(orderId);
    expect(reloaded?.getStatus()).toBe(OrderStatus.Cancelled);

    await client.disconnect();
  });
});
