import { randomUUID } from "crypto";

import {
  BaseDomainEvent,
  EventFactory,
  EventSerializer,
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


class UserRegistered extends BaseDomainEvent {
  readonly eventType = "UserRegistered" as const;
  readonly schemaVersion = 1 as const;

  constructor(public userId: string, public email: string, public name: string) {
    super();
  }
}

class UserEmailChanged extends BaseDomainEvent {
  readonly eventType = "UserEmailChanged" as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public userId: string,
    public previousEmail: string,
    public nextEmail: string,
  ) {
    super();
  }
}

async function main(): Promise<void> {
  const options = parseOptions();
  const client = await createClient(options);

  EventSerializer.registerEvent("UserRegistered", UserRegistered as unknown as new () => UserRegistered);
  EventSerializer.registerEvent("UserEmailChanged", UserEmailChanged as unknown as new () => UserEmailChanged);

  try {
    const userId = randomUUID();
    const streamName = `User-${userId}`;

    const registered = EventFactory.create(
      new UserRegistered(userId, "john@example.com", "John Doe"),
      {
        aggregateId: userId,
        aggregateType: "User",
        aggregateVersion: 1,
      },
    );

    const emailChanged = EventFactory.create(
      new UserEmailChanged(userId, "john@example.com", "john.doe@example.com"),
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
