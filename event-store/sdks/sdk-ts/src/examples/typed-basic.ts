import { EventStoreClientTS } from "../client";
import type { AppendRequest, ReadStreamRequest } from "../gen/eventstore/v1/eventstore";
import { Expected, EventMetadata } from "../gen/eventstore/v1/eventstore";

async function main() {
  const addr = process.env.EVENTSTORE_ADDR ?? "localhost:50051";
  const client = new EventStoreClientTS(addr);

  const aggregateId = `Order-TS-TYPED-${Date.now()}`;

  const appendReq: AppendRequest = {
    aggregateId,
    aggregateType: "Order",
    // Use Expected.NO_AGGREGATE to ensure we're creating a new aggregate
    expectedAny: Expected.NO_AGGREGATE,
    events: [
      {
        meta: EventMetadata.create({
          eventId: crypto.randomUUID(),
          aggregateId,
          aggregateType: "Order",
          aggregateNonce: 1, // First event in aggregate
          eventType: "OrderCreated",
          contentType: "text/plain",
        }),
        payload: Buffer.from("hello"),
      },
    ],
  };
  const appendResp = await client.append(appendReq);
  console.log("append:", appendResp);

  const readReq: ReadStreamRequest = {
    aggregateId,
    fromAggregateNonce: 1,
    maxCount: 100,
    forward: true,
  };
  const readResp = await client.readStream(readReq);
  console.log("read count:", readResp.events.length);

  for (const e of readResp.events) {
    const m = e.meta;
    console.log(
      `Event Type: ${m?.eventType}, Event ID: ${m?.eventId}, Aggregate ID: ${m?.aggregateId}, Aggregate Nonce: ${m?.aggregateNonce}`,
    );
  }
}

main().catch((e) => { console.error(e); process.exit(1); });
