import { EventStoreClientRT } from "../runtime-client";

async function main() {
  const addr = process.env.EVENTSTORE_ADDR ?? "localhost:50051";
  const client = new EventStoreClientRT(addr);

  const appendResp = await client.append({
    aggregate_id: "Order-TS-1",
    aggregate_type: "Order",
    expected_any: "ANY",
    events: [
      {
        meta: {
          aggregate_nonce: 1,  // client proposes the nonce
          event_type: "OrderCreated"
        },
        payload: Buffer.from("hello").toString("base64")
      },
    ],
  });
  console.log("append:", appendResp);

  const readResp = await client.readStream({
    aggregate_id: "Order-TS-1",
    from_aggregate_nonce: 1,
    max_count: 100,
    forward: true
  });
  console.log("read:", readResp);
}

main().catch((e) => { console.error(e); process.exit(1); });
