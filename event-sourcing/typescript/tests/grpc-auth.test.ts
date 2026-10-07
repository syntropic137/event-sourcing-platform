/**
 * Gateway credentials through the gRPC adapter (ADR-024, #302).
 *
 * An in-process gRPC server stands in for the nginx gateway: it answers
 * UNAUTHENTICATED unless the call carries the expected `authorization`
 * header, exactly as the gateway does.
 */
import path from 'path';
import * as grpc from '@grpc/grpc-js';
import * as protoLoader from '@grpc/proto-loader';

import { EventStoreAuthenticationError, EventStoreClientFactory, EventStoreError } from '../src';

const PROTO_PATH = path.resolve(
  __dirname,
  '..',
  '..',
  '..',
  'event-store',
  'eventstore-proto',
  'proto',
  'eventstore',
  'v1',
  'eventstore.proto'
);

const BASIC = `Basic ${Buffer.from('admin:s3cret').toString('base64')}`;

type Proto = {
  eventstore: { v1: { EventStore: { service: grpc.ServiceDefinition } } };
};

let server: grpc.Server;
let addr: string;
const seen: Array<string | undefined> = [];

beforeAll(async () => {
  const def = protoLoader.loadSync(PROTO_PATH, { longs: String, enums: String, defaults: true });
  const proto = grpc.loadPackageDefinition(def) as unknown as Proto;
  server = new grpc.Server();
  type Call = { metadata: grpc.Metadata };
  type Cb = (err: grpc.ServiceError | null, resp?: unknown) => void;
  const unary = (resp: unknown) => (call: Call, cb: Cb) => {
    const h = call.metadata.get('authorization')[0];
    seen.push(h === undefined ? undefined : String(h));
    if (h !== BASIC) {
      return cb({
        code: grpc.status.UNAUTHENTICATED,
        details: 'basic auth failed',
      } as grpc.ServiceError);
    }
    cb(null, resp);
  };
  server.addService(proto.eventstore.v1.EventStore.service, {
    ReadStream: unary({ events: [], isEnd: true, nextFromAggregateNonce: 0 }),
    ReadAll: unary({ events: [], isEnd: true, nextFromGlobalNonce: 0 }),
    Append: unary({ lastGlobalNonce: 1, lastAggregateNonce: 1 }),
  });
  const port = await new Promise<number>((resolve, reject) =>
    server.bindAsync('127.0.0.1:0', grpc.ServerCredentials.createInsecure(), (err, p) =>
      err ? reject(err) : resolve(p)
    )
  );
  addr = `127.0.0.1:${port}`;
});

afterAll(async () => {
  await new Promise<void>((resolve) => server.tryShutdown(() => resolve()));
});

beforeEach(() => {
  seen.length = 0;
});

describe('gRPC adapter gateway credentials', () => {
  it('sends basic auth on every call', async () => {
    const client = EventStoreClientFactory.createGrpcClient({
      serverAddress: addr,
      connection: { auth: { basic: { username: 'admin', password: 's3cret' } } },
    });
    await client.connect();
    await expect(client.readEvents('Order-1')).resolves.toEqual([]);
    await expect(client.streamExists('Order-1')).resolves.toBe(false);
    await expect(client.readAll(0, 10)).resolves.toMatchObject({ isEnd: true });
    expect(seen).toEqual([BASIC, BASIC, BASIC]);
  });

  it('maps rejected credentials to EventStoreAuthenticationError', async () => {
    const client = EventStoreClientFactory.createGrpcClient({
      serverAddress: addr,
      connection: { auth: { basic: { username: 'admin', password: 'wrong-pw' } } },
    });
    for (const call of [
      () => client.readEvents('Order-1'),
      () => client.readAll(0, 10),
      // Rejected credentials must not read as "stream does not exist".
      () => client.streamExists('Order-1'),
    ]) {
      const err = await call().then(
        () => undefined,
        (e: unknown) => e
      );
      expect(err).toBeInstanceOf(EventStoreAuthenticationError);
      expect(err).toBeInstanceOf(EventStoreError);
      expect((err as EventStoreError).code).toBe('EVENT_STORE_UNAUTHENTICATED');
      expect(JSON.stringify(err) + String(err)).not.toContain('wrong-pw');
    }
  });

  it('refuses credentials over plaintext to a non-loopback host', async () => {
    const client = EventStoreClientFactory.createGrpcClient({
      serverAddress: 'es.example.com:8081',
      connection: { auth: { bearerToken: 't' } },
    });
    await expect(client.connect()).rejects.toThrow(/plaintext/);
    await expect(client.readEvents('Order-1')).rejects.toThrow(/plaintext/);
    expect(seen).toEqual([]);
  });
});
