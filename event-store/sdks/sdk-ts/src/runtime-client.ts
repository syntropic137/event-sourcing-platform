import path from "node:path";
import { loadPackageDefinition } from "@grpc/grpc-js";
import { loadSync } from "@grpc/proto-loader";
import type { PackageDefinition } from "@grpc/proto-loader";
import { credentials } from "@grpc/grpc-js";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Minimal runtime-loaded client that works even before ts-proto stubs are generated
export class EventStoreClientRT {
  private readonly addr: string;
  private readonly client: any;

  constructor(addr: string) {
    this.addr = addr;
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
    this.client = new Svc(this.addr, credentials.createInsecure());
  }

  append(req: any): Promise<any> {
    return new Promise((resolve, reject) => {
      this.client.Append(req, (err: any, resp: any) => {
        if (err) return reject(err);
        resolve(resp);
      });
    });
  }

  readStream(req: any): Promise<any> {
    return new Promise((resolve, reject) => {
      this.client.ReadStream(req, (err: any, resp: any) => {
        if (err) return reject(err);
        resolve(resp);
      });
    });
  }

  subscribe(req: any): AsyncIterable<any> {
    const call = this.client.Subscribe(req);
    const iterator = {
      [Symbol.asyncIterator]() { return this; },
      next(): Promise<IteratorResult<any>> {
        return new Promise((resolve, reject) => {
          call.once("data", (data: any) => resolve({ value: data, done: false }));
          call.once("error", (err: any) => reject(err));
          call.once("end", () => resolve({ value: undefined, done: true }));
        });
      },
      return(): Promise<IteratorResult<any>> { call.cancel(); return Promise.resolve({ value: undefined, done: true }); }
    } as AsyncIterableIterator<any>;
    return iterator;
  }
}
