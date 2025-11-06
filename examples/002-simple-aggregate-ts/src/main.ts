import { randomUUID } from "crypto";
import {
  EventSerializer,
  EventStoreClient,
  EventStoreClientFactory,
  MemoryEventStoreClient,
  RepositoryFactory,
} from "@event-sourcing-platform/typescript";

// Infrastructure
import { CommandBus } from "./infrastructure/CommandBus";

// Slices (Adapters)
import { SubmitOrderCli } from "./slices/submit-order/SubmitOrderCli";
import { CancelOrderCli } from "./slices/cancel-order/CancelOrderCli";

// Domain Events (for registration)
import { OrderSubmittedEvent } from "./domain/events/OrderSubmittedEvent";
import { OrderCancelledEvent } from "./domain/events/OrderCancelledEvent";

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
        "   To use in-memory mode instead: rerun with --memory",
    );
    throw error;
  }
  return client;
}

/**
 * Main Entry Point
 * 
 * Demonstrates Hexagonal Event-Sourced VSA Architecture:
 * 
 * 1. Infrastructure Layer: EventStore, CommandBus, RepositoryFactory
 * 2. Domain Layer: OrderAggregate with business logic
 * 3. Adapter Layer: CLI slices (SubmitOrderCli, CancelOrderCli)
 * 
 * Flow: CLI Adapter → CommandBus → OrderAggregate → Events → EventStore
 * 
 * ADRs Demonstrated:
 * - ADR-004: Command handlers in aggregates
 * - ADR-006: Domain organization (domain/, slices/, infrastructure/)
 * - ADR-007: Event versioning (schemaVersion)
 * - ADR-008: Slices as thin hexagonal adapters
 */
async function main(): Promise<void> {
  console.log("🚀 002-simple-aggregate-ts (Hexagonal VSA Architecture)\n");

  // ============================================================================
  // Setup Infrastructure
  // ============================================================================

  const options = parseOptions();
  const client = await createClient(options);

  // Register events for serialization
  EventSerializer.registerEvent(
    "OrderSubmitted",
    OrderSubmittedEvent as unknown as new () => OrderSubmittedEvent,
  );
  EventSerializer.registerEvent(
    "OrderCancelled",
    OrderCancelledEvent as unknown as new () => OrderCancelledEvent,
  );

  // Create infrastructure services
  const repositoryFactory = new RepositoryFactory(client);
  const commandBus = new CommandBus(repositoryFactory);

  // ============================================================================
  // Create CLI Adapters (Hexagon Outside)
  // ============================================================================

  const submitOrderCli = new SubmitOrderCli(commandBus);
  const cancelOrderCli = new CancelOrderCli(commandBus);

  // ============================================================================
  // Execute Example Flow
  // ============================================================================

  try {
    const orderId = `order-${randomUUID()}`;

    console.log("📝 Executing order lifecycle...\n");

    // Step 1: Submit Order (via thin CLI adapter)
    console.log("1️⃣  Submit Order");
    await submitOrderCli.execute(orderId, "customer-xyz");
    console.log("");

    // Step 2: Cancel Order (via thin CLI adapter)
    console.log("2️⃣  Cancel Order");
    await cancelOrderCli.execute(orderId, "customer request");
    console.log("");

    console.log("✅ Example completed successfully!");
    console.log("");
    console.log("Architecture Highlights:");
    console.log("  • Domain layer (OrderAggregate) contains ALL business logic");
    console.log("  • CLI adapters are thin (< 50 lines, no business logic)");
    console.log("  • CommandBus routes commands to aggregates");
    console.log("  • Events are versioned (schemaVersion)");
    console.log("  • Slices are isolated (submit-order, cancel-order)");
    console.log("");
    console.log("Run 'vsa validate' to verify architecture compliance!");
  } finally {
    await client.disconnect();
  }
}

if (require.main === module) {
  main().catch((error) => {
    console.error("❌ Example failed", error);
    process.exitCode = 1;
  });
}

