import { GrpcEventStoreAdapter } from '../../src';

export interface CapturedAppend {
  tenantId: string;
  aggregateId: string;
  aggregateType: string;
  events: { meta: Record<string, unknown>; payload: Buffer }[];
}

/**
 * A real GrpcEventStoreAdapter whose gRPC client records `appendTyped`
 * requests. Built without the constructor, which loads the gRPC SDK.
 */
export function captureAdapter(tenantId: string): {
  adapter: GrpcEventStoreAdapter;
  sent: CapturedAppend[];
} {
  const sent: CapturedAppend[] = [];
  const adapter = Object.create(GrpcEventStoreAdapter.prototype) as GrpcEventStoreAdapter;
  Object.assign(adapter, {
    tenantId,
    clientPromise: Promise.resolve({
      appendTyped: async (req: CapturedAppend) => {
        sent.push(req);
      },
    }),
  });
  return { adapter, sent };
}
