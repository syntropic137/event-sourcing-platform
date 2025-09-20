import { randomUUID } from "crypto";

import {
  EventFactory,
  EventStoreClient,
  EventStoreClientFactory,
  MemoryEventStoreClient,
} from "@event-sourcing-platform/typescript";

type ClientMode = "memory" | "grpc";

type Options = {
  mode: ClientMode;
};

function parseOptions(): Options {
  if (process.argv.includes("--memory")) {
    return { mode: "memory" };
  }
  const envMode = (process.env.EVENT_STORE_MODE ?? "").toLowerCase();
  if (envMode === "memory") {
    return { mode: "memory" };
  }
  return { mode: "grpc" };
}

async function createClient(opts: Options): Promise<EventStoreClient> {
  if (opts.mode === "memory") {
    console.log(
      "🧪 Using in-memory event store client (override via --memory).",
    );
    const client = new MemoryEventStoreClient();
    await client.connect();
    return client;
  }

  const serverAddress = process.env.EVENT_STORE_ADDR ?? "127.0.0.1:50051";
  const tenantId = process.env.EVENT_STORE_TENANT ?? "example-tenant";
  console.log(
    `🛰️  Using gRPC event store at ${serverAddress} (tenant=${tenantId})`,
  );

  const client = EventStoreClientFactory.createGrpcClient({
    serverAddress,
    tenantId,
  });
  try {
    await client.connect();
  } catch (error) {
    console.error(
      "⚠️  Failed to connect to the gRPC event store.\n" +
      "   To start dev infrastructure: make dev-start\n" +
      "   To use in-memory mode instead: rerun with --memory"
    );
    throw error;
  }
  return client;
}

async function main(): Promise<void> {
  const options = parseOptions();
  const client = await createClient(options);

  try {
    const userId = randomUUID();
    const streamName = `User-${userId}`;

    const registered = EventFactory.create(
      {
        eventType: "UserRegistered",
        schemaVersion: 1,
        toJson: () => ({
          userId,
          email: "john@example.com",
          name: "John Doe",
          eventType: "UserRegistered",
          schemaVersion: 1,
        }),
      },
      {
        aggregateId: userId,
        aggregateType: "User",
        aggregateVersion: 1,
      },
    );

    const emailChanged = EventFactory.create(
      {
        eventType: "UserEmailChanged",
        schemaVersion: 1,
        toJson: () => ({
          userId,
          previousEmail: "john@example.com",
          nextEmail: "john.doe@example.com",
          eventType: "UserEmailChanged",
          schemaVersion: 1,
        }),
      },
      {
        aggregateId: userId,
        aggregateType: "User",
        aggregateVersion: 2,
      },
    );

    console.log(`📝 Appending initial registration to ${streamName}`);
    await client.appendEvents(streamName, [registered]);

    console.log(
      "📝 Appending email change with optimistic concurrency (expected version = 1)",
    );
    await client.appendEvents(streamName, [emailChanged], 1);

    console.log(`📖 Reading back the stream`);
    const envelopes = await client.readEvents(streamName);
    envelopes.forEach((envelope, idx) => {
      console.log(
        `  ${idx + 1}. ${envelope.event.eventType} (v${envelope.metadata.aggregateVersion})`,
      );
      console.log("     Payload:", envelope.event.toJson());
    });

    const exists = await client.streamExists(streamName);
    console.log(`🔍 Stream ${streamName} exists? ${exists}`);

    const ghostStream = await client.readEvents("User-non-existent");
    console.log(`📭 Non-existent stream returns ${ghostStream.length} events.`);
  } finally {
    await client.disconnect();
  }

  console.log("🎉 Example complete");
}

if (require.main === module) {
  main().catch((error) => {
    console.error("❌ Example failed", error);
    process.exitCode = 1;
  });
}
