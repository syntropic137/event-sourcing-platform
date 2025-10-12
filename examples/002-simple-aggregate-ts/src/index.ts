import { randomUUID } from "crypto";

import {
  Aggregate,
  AggregateRoot,
  BaseDomainEvent,
  CommandHandler,
  EventSourcingHandler,
  EventSerializer,
  EventStoreClient,
  EventStoreClientFactory,
  MemoryEventStoreClient,
  RepositoryFactory,
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

class OrderSubmitted extends BaseDomainEvent {
  readonly eventType = "OrderSubmitted" as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public orderId: string,
    public customerId: string,
  ) {
    super();
  }
}

class OrderCancelled extends BaseDomainEvent {
  readonly eventType = "OrderCancelled" as const;
  readonly schemaVersion = 1 as const;

  constructor(public reason: string) {
    super();
  }
}

// Commands
interface SubmitOrderCommand {
  aggregateId: string;
  orderId: string;
  customerId: string;
}

interface CancelOrderCommand {
  aggregateId: string;
  reason: string;
}

type OrderEvent = OrderSubmitted | OrderCancelled;

enum OrderStatus {
  New = "New",
  Submitted = "Submitted",
  Cancelled = "Cancelled",
}

@Aggregate("Order")
class OrderAggregate extends AggregateRoot<OrderEvent> {
  private status: OrderStatus = OrderStatus.New;

  @CommandHandler("SubmitOrderCommand")
  submit(command: SubmitOrderCommand): void {
    if (!this.aggregateId) {
      this.initialize(command.orderId);
    }
    if (this.status !== OrderStatus.New) {
      throw new Error(
        `Cannot submit order: Order is in '${this.status}' state, expected '${OrderStatus.New}'`
      );
    }
    this.apply(new OrderSubmitted(command.orderId, command.customerId));
  }

  @CommandHandler("CancelOrderCommand")
  cancel(command: CancelOrderCommand): void {
    if (this.status !== OrderStatus.Submitted) {
      throw new Error(
        `Cannot cancel order: Order is in '${this.status}' state, expected '${OrderStatus.Submitted}'`
      );
    }
    this.apply(new OrderCancelled(command.reason));
  }

  @EventSourcingHandler("OrderSubmitted")
  private onSubmitted(event: OrderSubmitted): void {
    if (!this.aggregateId) {
      this.initialize(event.orderId);
    }
    this.status = OrderStatus.Submitted;
  }

  @EventSourcingHandler("OrderCancelled")
  private onCancelled(): void {
    this.status = OrderStatus.Cancelled;
  }

  getStatus(): OrderStatus {
    return this.status;
  }
}

async function main(): Promise<void> {
  const options = parseOptions();
  const client = await createClient(options);

  EventSerializer.registerEvent(
    "OrderSubmitted",
    OrderSubmitted as unknown as new () => OrderSubmitted,
  );
  EventSerializer.registerEvent(
    "OrderCancelled",
    OrderCancelled as unknown as new () => OrderCancelled,
  );

  const repository = new RepositoryFactory(client).createRepository(
    () => new OrderAggregate(),
    "Order",
  );

  try {
    const orderId = `order-${randomUUID()}`;
    const aggregate = new OrderAggregate();
    aggregate.submit({
      aggregateId: orderId,
      orderId: orderId,
      customerId: "customer-xyz"
    });
    await repository.save(aggregate);
    console.log(`✅ Saved order ${orderId} at version ${aggregate.version}`);

    const loaded = await repository.load(orderId);
    console.log(`📦 Loaded order status: ${loaded?.getStatus()}`);

    loaded?.cancel({
      aggregateId: orderId,
      reason: "customer request"
    });
    if (loaded) {
      await repository.save(loaded);
    }

    const reloaded = await repository.load(orderId);
    console.log(`🔁 Reloaded order status: ${reloaded?.getStatus()}`);
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
