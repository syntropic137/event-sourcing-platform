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

// Customer Events
class CustomerRegistered extends BaseDomainEvent {
  readonly eventType = "CustomerRegistered" as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public customerId: string,
    public email: string,
    public name: string,
  ) {
    super();
  }
}

class CustomerEmailUpdated extends BaseDomainEvent {
  readonly eventType = "CustomerEmailUpdated" as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public previousEmail: string,
    public newEmail: string,
  ) {
    super();
  }
}

// Order Events
class OrderPlaced extends BaseDomainEvent {
  readonly eventType = "OrderPlaced" as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public orderId: string,
    public customerId: string,
    public items: Array<{ productId: string; quantity: number; price: number }>,
    public totalAmount: number,
  ) {
    super();
  }
}

class OrderShipped extends BaseDomainEvent {
  readonly eventType = "OrderShipped" as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public trackingNumber: string,
    public shippedAt: Date,
  ) {
    super();
  }
}

class OrderCancelled extends BaseDomainEvent {
  readonly eventType = "OrderCancelled" as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public reason: string,
    public cancelledAt: Date,
  ) {
    super();
  }
}

// Type unions for events
type CustomerEvent = CustomerRegistered | CustomerEmailUpdated;
type OrderEvent = OrderPlaced | OrderShipped | OrderCancelled;

// Customer Aggregate
enum CustomerStatus {
  Active = "Active",
  Inactive = "Inactive",
}

class CustomerAggregate extends AutoDispatchAggregate<CustomerEvent> {
  private email: string = "";
  private name: string = "";
  private status: CustomerStatus = CustomerStatus.Active;

  getAggregateType(): string {
    return "Customer";
  }

  register(customerId: string, email: string, name: string): void {
    if (this.id) {
      throw new Error("Customer already registered");
    }
    this.initialize(customerId);
    this.raiseEvent(new CustomerRegistered(customerId, email, name));
  }

  updateEmail(newEmail: string): void {
    if (!this.id) {
      throw new Error("Customer not registered");
    }
    if (this.email === newEmail) {
      throw new Error("Email is already set to this value");
    }
    this.raiseEvent(new CustomerEmailUpdated(this.email, newEmail));
  }

  @EventSourcingHandler("CustomerRegistered")
  private onRegistered(event: CustomerRegistered): void {
    this.email = event.email;
    this.name = event.name;
    this.status = CustomerStatus.Active;
  }

  @EventSourcingHandler("CustomerEmailUpdated")
  private onEmailUpdated(event: CustomerEmailUpdated): void {
    this.email = event.newEmail;
  }

  getEmail(): string {
    return this.email;
  }

  getName(): string {
    return this.name;
  }

  getStatus(): CustomerStatus {
    return this.status;
  }
}

// Order Aggregate
enum OrderStatus {
  Placed = "Placed",
  Shipped = "Shipped",
  Cancelled = "Cancelled",
}

class OrderAggregate extends AutoDispatchAggregate<OrderEvent> {
  private customerId: string = "";
  private items: Array<{ productId: string; quantity: number; price: number }> = [];
  private totalAmount: number = 0;
  private status: OrderStatus = OrderStatus.Placed;
  private trackingNumber?: string;

  getAggregateType(): string {
    return "Order";
  }

  place(
    orderId: string,
    customerId: string,
    items: Array<{ productId: string; quantity: number; price: number }>,
  ): void {
    if (this.id) {
      throw new Error("Order already placed");
    }
    if (items.length === 0) {
      throw new Error("Order must have at least one item");
    }
    
    this.initialize(orderId);
    const totalAmount = items.reduce((sum, item) => sum + (item.quantity * item.price), 0);
    this.raiseEvent(new OrderPlaced(orderId, customerId, items, totalAmount));
  }

  ship(trackingNumber: string): void {
    if (!this.id) {
      throw new Error("Order not placed");
    }
    if (this.status !== OrderStatus.Placed) {
      throw new Error(`Cannot ship order in status: ${this.status}`);
    }
    this.raiseEvent(new OrderShipped(trackingNumber, new Date()));
  }

  cancel(reason: string): void {
    if (!this.id) {
      throw new Error("Order not placed");
    }
    if (this.status === OrderStatus.Shipped) {
      throw new Error("Cannot cancel shipped order");
    }
    if (this.status === OrderStatus.Cancelled) {
      throw new Error("Order is already cancelled");
    }
    this.raiseEvent(new OrderCancelled(reason, new Date()));
  }

  @EventSourcingHandler("OrderPlaced")
  private onPlaced(event: OrderPlaced): void {
    this.customerId = event.customerId;
    this.items = [...event.items];
    this.totalAmount = event.totalAmount;
    this.status = OrderStatus.Placed;
  }

  @EventSourcingHandler("OrderShipped")
  private onShipped(event: OrderShipped): void {
    this.status = OrderStatus.Shipped;
    this.trackingNumber = event.trackingNumber;
  }

  @EventSourcingHandler("OrderCancelled")
  private onCancelled(): void {
    this.status = OrderStatus.Cancelled;
  }

  getCustomerId(): string {
    return this.customerId;
  }

  getItems(): Array<{ productId: string; quantity: number; price: number }> {
    return [...this.items];
  }

  getTotalAmount(): number {
    return this.totalAmount;
  }

  getStatus(): OrderStatus {
    return this.status;
  }

  getTrackingNumber(): string | undefined {
    return this.trackingNumber;
  }
}

async function main(): Promise<void> {
  const options = parseOptions();
  const client = await createClient(options);

  // Register all event types
  EventSerializer.registerEvent(
    "CustomerRegistered",
    CustomerRegistered as unknown as new () => CustomerRegistered,
  );
  EventSerializer.registerEvent(
    "CustomerEmailUpdated",
    CustomerEmailUpdated as unknown as new () => CustomerEmailUpdated,
  );
  EventSerializer.registerEvent(
    "OrderPlaced",
    OrderPlaced as unknown as new () => OrderPlaced,
  );
  EventSerializer.registerEvent(
    "OrderShipped",
    OrderShipped as unknown as new () => OrderShipped,
  );
  EventSerializer.registerEvent(
    "OrderCancelled",
    OrderCancelled as unknown as new () => OrderCancelled,
  );

  // Create repositories
  const repositoryFactory = new RepositoryFactory(client);
  const customerRepository = repositoryFactory.createRepository(
    () => new CustomerAggregate(),
    "Customer",
  );
  const orderRepository = repositoryFactory.createRepository(
    () => new OrderAggregate(),
    "Order",
  );

  try {
    console.log("🏪 Multiple Aggregates Example: Customer & Order Management");
    console.log("=========================================================");

    // 1. Register a customer
    const customerId = `customer-${randomUUID()}`;
    const customer = new CustomerAggregate();
    customer.register(customerId, "alice@example.com", "Alice Johnson");
    await customerRepository.save(customer);
    console.log(`✅ Registered customer ${customerId} (${customer.getName()}) at version ${customer.version}`);

    // 2. Update customer email
    customer.updateEmail("alice.johnson@example.com");
    await customerRepository.save(customer);
    console.log(`📧 Updated customer email to ${customer.getEmail()} at version ${customer.version}`);

    // 3. Place an order for the customer
    const orderId = `order-${randomUUID()}`;
    const order = new OrderAggregate();
    order.place(orderId, customerId, [
      { productId: "laptop-001", quantity: 1, price: 999.99 },
      { productId: "mouse-001", quantity: 2, price: 29.99 },
    ]);
    await orderRepository.save(order);
    console.log(`🛒 Placed order ${orderId} for customer ${order.getCustomerId()}`);
    console.log(`   Total: $${order.getTotalAmount().toFixed(2)} (${order.getItems().length} item types)`);

    // 4. Ship the order
    order.ship("TRACK123456789");
    await orderRepository.save(order);
    console.log(`📦 Shipped order ${orderId} with tracking: ${order.getTrackingNumber()}`);

    // 5. Place another order and cancel it
    const orderId2 = `order-${randomUUID()}`;
    const order2 = new OrderAggregate();
    order2.place(orderId2, customerId, [
      { productId: "keyboard-001", quantity: 1, price: 149.99 },
    ]);
    await orderRepository.save(order2);
    console.log(`🛒 Placed second order ${orderId2} for $${order2.getTotalAmount().toFixed(2)}`);

    order2.cancel("Customer changed mind");
    await orderRepository.save(order2);
    console.log(`❌ Cancelled order ${orderId2}: ${order2.getStatus()}`);

    // 6. Load aggregates to verify state
    console.log("\n📖 Verifying final state by reloading from event store:");
    
    const loadedCustomer = await customerRepository.load(customerId);
    if (loadedCustomer) {
      console.log(`👤 Customer: ${loadedCustomer.getName()} (${loadedCustomer.getEmail()}) - ${loadedCustomer.getStatus()}`);
    }

    const loadedOrder1 = await orderRepository.load(orderId);
    if (loadedOrder1) {
      console.log(`📦 Order 1: ${loadedOrder1.getStatus()} - $${loadedOrder1.getTotalAmount().toFixed(2)} - Tracking: ${loadedOrder1.getTrackingNumber()}`);
    }

    const loadedOrder2 = await orderRepository.load(orderId2);
    if (loadedOrder2) {
      console.log(`📦 Order 2: ${loadedOrder2.getStatus()} - $${loadedOrder2.getTotalAmount().toFixed(2)}`);
    }

    console.log("\n🎉 Multiple aggregates example completed successfully!");
    console.log("💡 This example demonstrates:");
    console.log("   • Multiple aggregate types (Customer, Order)");
    console.log("   • Cross-aggregate relationships (Order references Customer)");
    console.log("   • Independent aggregate lifecycles");
    console.log("   • Event sourcing with multiple event types");

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
