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
import { Expected, EventMetadata } from "./gen/eventstore/v1/eventstore.js";

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
    aggregateId: string;
    aggregateType: string;
    exact?: number;
    expectedAny?: Expected;
    events: Array<{
      meta: {
        aggregateNonce: number;  // client MUST provide this for optimistic concurrency
        eventType: string;
        eventId?: string;
        contentType?: string;
        correlationId?: string;
        causationId?: string;
        actorId?: string;
        tenantId?: string;
        headers?: Record<string, string>;
        schemaVersion?: number;
      };
      payload: Buffer;
    }>;
  }): Promise<AppendResponse> {
    const { aggregateId, aggregateType, exact, expectedAny, events } = input;
    const wireReq: AppendRequest = {
      aggregateId,
      aggregateType,
      exact,
      expectedAny,
      events: events.map((e) => ({
        meta: EventMetadata.create({
          aggregateId,
          aggregateType,
          aggregateNonce: e.meta.aggregateNonce,  // client provides for optimistic concurrency
          eventType: e.meta.eventType,
          eventId: e.meta.eventId ?? "",
          contentType: e.meta.contentType ?? "application/octet-stream",
          correlationId: e.meta.correlationId ?? "",
          causationId: e.meta.causationId ?? "",
          actorId: e.meta.actorId ?? "",
          tenantId: e.meta.tenantId ?? "",
          headers: e.meta.headers ?? {},
          schemaVersion: e.meta.schemaVersion ?? 0,
          // globalNonce/timestamp set by store
        }),
        payload: e.payload,
      })),
    };
    return this.append(wireReq);
  }
}

