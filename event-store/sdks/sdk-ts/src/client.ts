import { credentials, ChannelCredentials, Client } from "@grpc/grpc-js";
import type { EventStoreClient as GrpcClient } from "./gen/eventstore/v1/eventstore.js";
import { EventStoreClient as GrpcClientCtor } from "./gen/eventstore/v1/eventstore.js";
import type {
  AppendRequest,
  AppendResponse,
  ReadStreamRequest,
  ReadStreamResponse,
  SubscribeRequest,
  SubscribeResponse,
} from "./gen/eventstore/v1/eventstore.js";
import { EventMetadata } from "./gen/eventstore/v1/eventstore.js";

export interface ClientOptions {
  /** plaintext by default */
  credentials?: ChannelCredentials;
}

export class EventStoreClientTS {
  private readonly client: GrpcClient & Client;

  constructor(addr: string, opts: ClientOptions = {}) {
    const creds = opts.credentials ?? credentials.createInsecure();
    // Generated ctor is typed to return EventStoreClient
    this.client = new GrpcClientCtor(addr, creds) as GrpcClient & Client;
  }

  append(req: AppendRequest): Promise<AppendResponse> {
    return new Promise((resolve, reject) => {
      this.client.append(req, (err, resp) => {
        if (err) return reject(err);
        resolve(resp);
      });
    });
  }

  readStream(req: ReadStreamRequest): Promise<ReadStreamResponse> {
    return new Promise((resolve, reject) => {
      this.client.readStream(req, (err, resp) => {
        if (err) return reject(err);
        resolve(resp);
      });
    });
  }

  subscribe(req: SubscribeRequest): AsyncIterable<SubscribeResponse> {
    const call = this.client.subscribe(req);
    const iterator = {
      [Symbol.asyncIterator]() { return this; },
      next(): Promise<IteratorResult<SubscribeResponse>> {
        return new Promise((resolve, reject) => {
          call.once("data", (data: SubscribeResponse) => resolve({ value: data, done: false }));
          call.once("error", (err: unknown) => reject(err));
          call.once("end", () => resolve({ done: true } as IteratorReturnResult<SubscribeResponse>));
        });
      },
      return(): Promise<IteratorResult<SubscribeResponse>> {
        call.cancel();
        return Promise.resolve({ done: true } as IteratorReturnResult<SubscribeResponse>);
      }
    } as AsyncIterableIterator<SubscribeResponse>;
    return iterator;
  }

  // High-level, fully typed append that requires event metadata
  appendTyped(input: {
    tenantId: string;
    aggregateId: string;
    aggregateType: string;
    expectedAggregateNonce: number;
    idempotencyKey?: string;
    events: Array<{
      meta: {
        aggregateNonce: number;  // client MUST provide this for optimistic concurrency
        eventType: string;
        eventVersion?: number;
        eventId?: string;
        contentType?: string;
        contentSchema?: string;
        correlationId?: string;
        causationId?: string;
        actorId?: string;
        tenantId?: string;
        timestampUnixMs?: number;
        payloadSha256?: Uint8Array;
        headers?: Record<string, string>;
      };
      payload: Buffer;
    }>;
  }): Promise<AppendResponse> {
    const { tenantId, aggregateId, aggregateType, expectedAggregateNonce, idempotencyKey, events } = input;
    const wireReq: AppendRequest = {
      tenantId,
      aggregateId,
      aggregateType,
      expectedAggregateNonce,
      idempotencyKey: idempotencyKey ?? "",
      events: events.map((e) => ({
        meta: EventMetadata.create({
          aggregateId,
          aggregateType,
          aggregateNonce: e.meta.aggregateNonce,  // client provides for optimistic concurrency
          eventType: e.meta.eventType,
          eventVersion: e.meta.eventVersion ?? 1,
          eventId: e.meta.eventId ?? "",
          contentType: e.meta.contentType ?? "application/octet-stream",
          contentSchema: e.meta.contentSchema ?? "",
          correlationId: e.meta.correlationId ?? "",
          causationId: e.meta.causationId ?? "",
          actorId: e.meta.actorId ?? "",
          tenantId: e.meta.tenantId ?? tenantId,
          timestampUnixMs: e.meta.timestampUnixMs ?? 0,
          recordedTimeUnixMs: 0,
          payloadSha256: e.meta.payloadSha256
            ? Buffer.from(e.meta.payloadSha256)
            : Buffer.alloc(0),
          headers: e.meta.headers ?? {},
          globalNonce: 0,
        }),
        payload: e.payload,
      })),
    };
    return this.append(wireReq);
  }
}
