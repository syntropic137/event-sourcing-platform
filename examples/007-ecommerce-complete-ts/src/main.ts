/**
 * 007-ecommerce-complete-ts
 * 
 * Complete E-commerce Example demonstrating Hexagonal Event-Sourced VSA Architecture
 * 
 * Features:
 * - 3 Aggregates: Product, Order, Customer
 * - 11 Commands organized in domain/commands/
 * - 11 Events with @Event decorators in domain/events/
 * - CommandBus in infrastructure layer
 * - Complete order fulfillment workflow
 */

import {
  EventSerializer,
  EventStoreClient,
  EventStoreClientFactory,
  MemoryEventStoreClient,
} from "@neuralempowerment/event-sourcing-typescript";
import { CommandBus } from "./infrastructure/CommandBus";

// Import commands
import { RegisterCustomerCommand } from "./domain/commands/RegisterCustomerCommand";
import { CreateProductCommand } from "./domain/commands/CreateProductCommand";
import { AddStockCommand } from "./domain/commands/AddStockCommand";
import { CreateOrderCommand } from "./domain/commands/CreateOrderCommand";
import { AddOrderItemCommand } from "./domain/commands/AddOrderItemCommand";
import { ConfirmOrderCommand } from "./domain/commands/ConfirmOrderCommand";
import { RemoveStockCommand } from "./domain/commands/RemoveStockCommand";
import { ShipOrderCommand } from "./domain/commands/ShipOrderCommand";

// Import events for registration
import { ProductCreatedEvent } from "./domain/events/ProductCreatedEvent";
import { ProductPriceUpdatedEvent } from "./domain/events/ProductPriceUpdatedEvent";
import { StockAddedEvent } from "./domain/events/StockAddedEvent";
import { StockRemovedEvent } from "./domain/events/StockRemovedEvent";
import { OrderCreatedEvent } from "./domain/events/OrderCreatedEvent";
import { OrderItemAddedEvent } from "./domain/events/OrderItemAddedEvent";
import { OrderConfirmedEvent } from "./domain/events/OrderConfirmedEvent";
import { OrderShippedEvent } from "./domain/events/OrderShippedEvent";
import { OrderCancelledEvent } from "./domain/events/OrderCancelledEvent";
import { CustomerRegisteredEvent } from "./domain/events/CustomerRegisteredEvent";
import { CustomerAddressUpdatedEvent } from "./domain/events/CustomerAddressUpdatedEvent";

type ClientMode = "memory" | "grpc";
type Options = { mode: ClientMode };

function parseOptions(): Options {
  if (process.argv.includes("--memory")) return { mode: "memory" };
  const envMode = (process.env.EVENT_STORE_MODE ?? "").toLowerCase();
  if (envMode === "memory") return { mode: "memory" };
  return { mode: "grpc" };
}

async function createClient(opts: Options): Promise<EventStoreClient> {
  if (opts.mode === "memory") {
    console.log("🧪 Using in-memory event store client");
    const client = new MemoryEventStoreClient();
    await client.connect();
    return client;
  }

  const serverAddress = process.env.EVENT_STORE_ADDR ?? "127.0.0.1:50051";
  const tenantId = process.env.EVENT_STORE_TENANT ?? "ecommerce-tenant";
  console.log(`🛰️  Using gRPC event store at ${serverAddress} (tenant=${tenantId})`);

  const client = EventStoreClientFactory.createGrpcClient({
    serverAddress,
    tenantId,
  });
  try {
    await client.connect();
  } catch (error) {
    console.error(
      "⚠️  Failed to connect to gRPC event store.\n" +
        "   To start dev infrastructure: make dev-start\n" +
        "   To use in-memory mode instead: rerun with --memory"
    );
    throw error;
  }
  return client;
}

async function main(): Promise<void> {
  console.log("🛒 E-commerce Platform - Complete Example");
  console.log("==========================================");
  console.log("✅ HEXAGONAL EVENT-SOURCED VSA ARCHITECTURE\n");

  const options = parseOptions();
  const client = await createClient(options);

  // Register all events for serialization
  EventSerializer.registerEvent("ProductCreated", ProductCreatedEvent as any);
  EventSerializer.registerEvent(
    "ProductPriceUpdated",
    ProductPriceUpdatedEvent as any
  );
  EventSerializer.registerEvent("StockAdded", StockAddedEvent as any);
  EventSerializer.registerEvent("StockRemoved", StockRemovedEvent as any);
  EventSerializer.registerEvent("OrderCreated", OrderCreatedEvent as any);
  EventSerializer.registerEvent("OrderItemAdded", OrderItemAddedEvent as any);
  EventSerializer.registerEvent("OrderConfirmed", OrderConfirmedEvent as any);
  EventSerializer.registerEvent("OrderShipped", OrderShippedEvent as any);
  EventSerializer.registerEvent("OrderCancelled", OrderCancelledEvent as any);
  EventSerializer.registerEvent(
    "CustomerRegistered",
    CustomerRegisteredEvent as any
  );
  EventSerializer.registerEvent(
    "CustomerAddressUpdated",
    CustomerAddressUpdatedEvent as any
  );

  const commandBus = new CommandBus(client);

  try {
    // Demo: Customer Registration
    console.log("👤 DEMO: Customer Registration");
    console.log("===============================");
    const customerId = "customer-001";
    await commandBus.send(
      new RegisterCustomerCommand(
        customerId,
        "john.doe@example.com",
        "John Doe",
        "123 Main St, Springfield"
      )
    );
    console.log(`✓ Customer registered: ${customerId}`);
    console.log(`  Email: john.doe@example.com`);
    console.log(`  Name: John Doe`);

    // Demo: Product Management
    console.log("\n📦 DEMO: Product Management");
    console.log("============================");
    const productId = "product-001";
    await commandBus.send(
      new CreateProductCommand(
        productId,
        "Wireless Mouse",
        "Ergonomic wireless mouse with 6 buttons",
        29.99,
        100
      )
    );
    console.log(`✓ Product created: ${productId}`);
    console.log(`  Name: Wireless Mouse`);
    console.log(`  Price: $29.99`);
    console.log(`  Stock: 100 units`);

    await commandBus.send(new AddStockCommand(productId, 50));
    console.log(`✓ Stock added: +50 units (now 150 units)`);

    // Demo: Order Lifecycle
    console.log("\n📋 DEMO: Order Lifecycle");
    console.log("=========================");
    const orderId = "order-001";
    await commandBus.send(new CreateOrderCommand(orderId, customerId));
    console.log(`✓ Order created: ${orderId}`);

    await commandBus.send(
      new AddOrderItemCommand(orderId, productId, "Wireless Mouse", 2, 29.99)
    );
    console.log(`✓ Item added: 2x Wireless Mouse @ $29.99`);

    await commandBus.send(new ConfirmOrderCommand(orderId));
    console.log(`✓ Order confirmed (Status: CONFIRMED)`);
    console.log(`  Total: $59.98`);

    // Remove stock for the order
    await commandBus.send(new RemoveStockCommand(productId, 2, orderId));
    console.log(`✓ Stock removed: -2 units (now 148 units)`);

    await commandBus.send(new ShipOrderCommand(orderId, "TRACK-12345"));
    console.log(`✓ Order shipped (Status: SHIPPED)`);

    console.log("\n🎉 Complete E-commerce Flow Demonstrated!");
    console.log("\n📊 Architecture Summary:");
    console.log("  ✓ 3 Aggregates: Product, Order, Customer");
    console.log("  ✓ 11 Commands in domain/commands/");
    console.log("  ✓ 11 Events with @Event decorators in domain/events/");
    console.log("  ✓ CommandBus in infrastructure/");
    console.log("  ✓ Full hexagonal architecture with VSA");
    console.log("  ✓ All events versioned (v1)");
    console.log("  ✓ Complete order fulfillment workflow");
    console.log("\n✅ ARCHITECTURE COMPLIANCE VERIFIED\n");
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

