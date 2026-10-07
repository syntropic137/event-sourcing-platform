import path from "node:path";
import { loadPackageDefinition } from "@grpc/grpc-js";
import { loadSync } from "@grpc/proto-loader";
import type { PackageDefinition } from "@grpc/proto-loader";
import { mapGrpcError, resolveConnection, type ConnectionOptions } from "./auth.js";
import { fileURLToPath } from "node:url";
import { inspect } from "node:util";
import { streamToAsyncIterator } from "./stream-iterator.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Minimal runtime-loaded client that works even before ts-proto stubs are generated
export class EventStoreClientRT {
  private readonly addr: string;
  // ES private field: never shown by util.inspect (in-flight calls hold
  // the authorization header).
  readonly #client: any;

  /** Same endpoint forms and options as `EventStoreClientTS`. */
  constructor(addr: string, opts: ConnectionOptions = {}) {
    const conn = resolveConnection(addr, opts);
    this.addr = conn.target;
    const protoPath = path.resolve(__dirname, "../../../eventstore-proto/proto/eventstore/v1/eventstore.proto");
    const def: PackageDefinition = loadSync(protoPath, {
      keepCase: true,
      longs: String,
      enums: String,
      defaults: true,
      oneofs: true,
      includeDirs: [path.resolve(__dirname, "../../../eventstore-proto/proto")],
    });
    const pkg = loadPackageDefinition(def) as any;
    const Svc = pkg.eventstore.v1.EventStore;
    this.#client = new Svc(conn.target, conn.channelCredentials, conn.options);
  }

  /** Never includes credentials. */
  toString(): string {
    return "EventStoreClientRT";
  }
  toJSON(): string {
    return this.toString();
  }
  [inspect.custom](): string {
    return this.toString();
  }

  append(req: any): Promise<any> {
    return new Promise((resolve, reject) => {
      this.#client.Append(req, (err: any, resp: any) => {
        if (err) return reject(mapGrpcError(err));
        resolve(resp);
      });
    });
  }

  readStream(req: any): Promise<any> {
    return new Promise((resolve, reject) => {
      this.#client.ReadStream(req, (err: any, resp: any) => {
        if (err) return reject(mapGrpcError(err));
        resolve(resp);
      });
    });
  }

  subscribe(req: any): AsyncIterable<any> {
    return streamToAsyncIterator<any>(this.#client.Subscribe(req), mapGrpcError);
  }
}
