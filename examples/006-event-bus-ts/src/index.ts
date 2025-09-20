import { randomUUID } from "crypto";

import {
  AutoDispatchAggregate,
  BaseDomainEvent,
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

// Domain Events
class UserRegistered extends BaseDomainEvent {
  readonly eventType = "UserRegistered" as const;
  readonly schemaVersion = 1 as const;
  constructor(public userId: string, public email: string, public name: string) { super(); }
}

class OrderPlaced extends BaseDomainEvent {
  readonly eventType = "OrderPlaced" as const;
  readonly schemaVersion = 1 as const;
  constructor(public orderId: string, public userId: string, public amount: number) { super(); }
}

class PaymentProcessed extends BaseDomainEvent {
  readonly eventType = "PaymentProcessed" as const;
  readonly schemaVersion = 1 as const;
  constructor(public paymentId: string, public orderId: string, public amount: number) { super(); }
}

class NotificationSent extends BaseDomainEvent {
  readonly eventType = "NotificationSent" as const;
  readonly schemaVersion = 1 as const;
  constructor(public userId: string, public type: string, public message: string) { super(); }
}

// Aggregates
class UserAggregate extends AutoDispatchAggregate<UserRegistered> {
  private email = ""; private name = "";
  getAggregateType() { return "User"; }
  
  register(id: string, email: string, name: string) {
    this.initialize(id);
    this.raiseEvent(new UserRegistered(id, email, name));
  }

  @EventSourcingHandler("UserRegistered")
  onRegistered(e: UserRegistered) { this.email = e.email; this.name = e.name; }
}

class OrderAggregate extends AutoDispatchAggregate<OrderPlaced> {
  private userId = ""; private amount = 0;
  getAggregateType() { return "Order"; }
  
  place(id: string, userId: string, amount: number) {
    this.initialize(id);
    this.raiseEvent(new OrderPlaced(id, userId, amount));
  }

  @EventSourcingHandler("OrderPlaced")
  onPlaced(e: OrderPlaced) { this.userId = e.userId; this.amount = e.amount; }
}

class PaymentAggregate extends AutoDispatchAggregate<PaymentProcessed> {
  getAggregateType() { return "Payment"; }
  
  process(id: string, orderId: string, amount: number) {
    this.initialize(id);
    this.raiseEvent(new PaymentProcessed(id, orderId, amount));
  }

  @EventSourcingHandler("PaymentProcessed")
  onProcessed() { /* state updates */ }
}

class NotificationAggregate extends AutoDispatchAggregate<NotificationSent> {
  getAggregateType() { return "Notification"; }
  
  send(id: string, userId: string, type: string, message: string) {
    this.initialize(id);
    this.raiseEvent(new NotificationSent(userId, type, message));
  }

  @EventSourcingHandler("NotificationSent")
  onSent() { /* state updates */ }
}

// Event Bus
interface EventHandler {
  handle(event: any): Promise<void>;
}

class EventBus {
  private handlers = new Map<string, EventHandler[]>();

  subscribe(eventType: string, handler: EventHandler) {
    if (!this.handlers.has(eventType)) {
      this.handlers.set(eventType, []);
    }
    this.handlers.get(eventType)!.push(handler);
  }

  async publish(event: any) {
    const handlers = this.handlers.get(event.eventType) || [];
    console.log(`📡 Publishing ${event.eventType} to ${handlers.length} handlers`);
    
    for (const handler of handlers) {
      try {
        await handler.handle(event);
      } catch (error) {
        console.error(`Handler error for ${event.eventType}:`, error);
      }
    }
  }
}

// Event Handlers
class PaymentHandler implements EventHandler {
  constructor(private paymentRepo: any) {}
  
  async handle(event: any) {
    if (event.eventType === "OrderPlaced") {
      const paymentId = `payment-${randomUUID()}`;
      const payment = new PaymentAggregate();
      payment.process(paymentId, event.orderId, event.amount);
      await this.paymentRepo.save(payment);
      console.log(`💳 Processed payment ${paymentId} for order ${event.orderId}`);
    }
  }
}

class NotificationHandler implements EventHandler {
  constructor(private notificationRepo: any) {}
  
  async handle(event: any) {
    const notificationId = `notification-${randomUUID()}`;
    const notification = new NotificationAggregate();
    
    switch (event.eventType) {
      case "UserRegistered":
        notification.send(notificationId, event.userId, "welcome", `Welcome ${event.name}!`);
        await this.notificationRepo.save(notification);
        console.log(`📧 Sent welcome notification to ${event.name}`);
        break;
      case "OrderPlaced":
        notification.send(notificationId, event.userId, "order", `Order ${event.orderId} placed for $${event.amount}`);
        await this.notificationRepo.save(notification);
        console.log(`📧 Sent order confirmation to user ${event.userId}`);
        break;
      case "PaymentProcessed":
        // Would need to look up user from order, simplified here
        notification.send(notificationId, "user", "payment", `Payment processed for $${event.amount}`);
        await this.notificationRepo.save(notification);
        console.log(`📧 Sent payment confirmation`);
        break;
    }
  }
}

async function main(): Promise<void> {
  const options = parseOptions();
  const client = await createClient(options);

  EventSerializer.registerEvent("UserRegistered", UserRegistered as any);
  EventSerializer.registerEvent("OrderPlaced", OrderPlaced as any);
  EventSerializer.registerEvent("PaymentProcessed", PaymentProcessed as any);
  EventSerializer.registerEvent("NotificationSent", NotificationSent as any);

  const factory = new RepositoryFactory(client);
  const userRepo = factory.createRepository(() => new UserAggregate(), "User");
  const orderRepo = factory.createRepository(() => new OrderAggregate(), "Order");
  const paymentRepo = factory.createRepository(() => new PaymentAggregate(), "Payment");
  const notificationRepo = factory.createRepository(() => new NotificationAggregate(), "Notification");

  // Set up event bus
  const eventBus = new EventBus();
  eventBus.subscribe("OrderPlaced", new PaymentHandler(paymentRepo));
  eventBus.subscribe("UserRegistered", new NotificationHandler(notificationRepo));
  eventBus.subscribe("OrderPlaced", new NotificationHandler(notificationRepo));
  eventBus.subscribe("PaymentProcessed", new NotificationHandler(notificationRepo));

  try {
    console.log("🚌 Event Bus Example: Cross-Aggregate Communication");
    console.log("==================================================");

    // Register user
    const userId = `user-${randomUUID()}`;
    const user = new UserAggregate();
    user.register(userId, "alice@example.com", "Alice");
    await userRepo.save(user);
    console.log(`👤 Registered user ${userId}`);

    // Publish user registration event
    await eventBus.publish(new UserRegistered(userId, "alice@example.com", "Alice"));

    // Place order
    const orderId = `order-${randomUUID()}`;
    const order = new OrderAggregate();
    order.place(orderId, userId, 199.99);
    await orderRepo.save(order);
    console.log(`🛒 Placed order ${orderId}`);

    // Publish order placed event (triggers payment and notification)
    await eventBus.publish(new OrderPlaced(orderId, userId, 199.99));

    // Simulate payment completion and publish event
    await eventBus.publish(new PaymentProcessed(`payment-${randomUUID()}`, orderId, 199.99));

    console.log("\n📊 Event Flow Summary:");
    console.log("1. User registered → Welcome notification sent");
    console.log("2. Order placed → Payment processed + Order notification sent");
    console.log("3. Payment processed → Payment confirmation sent");

    console.log("\n🎉 Event Bus example completed!");
    console.log("💡 Demonstrates: Cross-aggregate communication, event-driven architecture, decoupled handlers");

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
