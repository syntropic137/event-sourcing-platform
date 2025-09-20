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

// Events
class ProductCreated extends BaseDomainEvent {
  readonly eventType = "ProductCreated" as const;
  readonly schemaVersion = 1 as const;
  constructor(public productId: string, public name: string, public price: number, public stock: number) { super(); }
}

class ProductSold extends BaseDomainEvent {
  readonly eventType = "ProductSold" as const;
  readonly schemaVersion = 1 as const;
  constructor(public quantity: number, public unitPrice: number, public orderId: string) { super(); }
}

class OrderPlaced extends BaseDomainEvent {
  readonly eventType = "OrderPlaced" as const;
  readonly schemaVersion = 1 as const;
  constructor(public orderId: string, public customerId: string, public totalAmount: number) { super(); }
}

// Aggregates
class ProductAggregate extends AutoDispatchAggregate<ProductCreated | ProductSold> {
  private name = ""; private price = 0; private stock = 0;
  getAggregateType() { return "Product"; }
  
  create(id: string, name: string, price: number, stock: number) {
    this.initialize(id);
    this.raiseEvent(new ProductCreated(id, name, price, stock));
  }
  
  sell(quantity: number, orderId: string) {
    if (this.stock < quantity) throw new Error("Insufficient stock");
    this.raiseEvent(new ProductSold(quantity, this.price, orderId));
  }

  @EventSourcingHandler("ProductCreated")
  onCreated(e: ProductCreated) { this.name = e.name; this.price = e.price; this.stock = e.stock; }
  
  @EventSourcingHandler("ProductSold")
  onSold(e: ProductSold) { this.stock -= e.quantity; }
}

class OrderAggregate extends AutoDispatchAggregate<OrderPlaced> {
  getAggregateType() { return "Order"; }
  
  place(id: string, customerId: string, amount: number) {
    this.initialize(id);
    this.raiseEvent(new OrderPlaced(id, customerId, amount));
  }

  @EventSourcingHandler("OrderPlaced")
  onPlaced() { /* state updates */ }
}

// Projections
interface SalesReport { totalOrders: number; totalRevenue: number; topProducts: Array<{id: string; name: string; sold: number}>; }
interface ProductCatalog { products: Array<{id: string; name: string; price: number; stock: number; totalSold: number}>; }

class SalesProjection {
  private report: SalesReport = { totalOrders: 0, totalRevenue: 0, topProducts: [] };
  
  processEvent(envelope: any) {
    const event = envelope.event;
    switch (event.eventType) {
      case "OrderPlaced":
        this.report.totalOrders++;
        this.report.totalRevenue += event.totalAmount;
        break;
      case "ProductSold":
        let product = this.report.topProducts.find(p => p.id === envelope.metadata.aggregateId);
        if (!product) {
          product = { id: envelope.metadata.aggregateId, name: "Product", sold: 0 };
          this.report.topProducts.push(product);
        }
        product.sold += event.quantity;
        break;
    }
  }
  
  getReport() { return this.report; }
}

class CatalogProjection {
  private catalog: ProductCatalog = { products: [] };
  
  processEvent(envelope: any) {
    const event = envelope.event;
    const productId = envelope.metadata.aggregateId;
    
    switch (event.eventType) {
      case "ProductCreated":
        this.catalog.products.push({
          id: productId, name: event.name, price: event.price, stock: event.stock, totalSold: 0
        });
        break;
      case "ProductSold":
        const product = this.catalog.products.find(p => p.id === productId);
        if (product) {
          product.stock -= event.quantity;
          product.totalSold += event.quantity;
        }
        break;
    }
  }
  
  getCatalog() { return this.catalog; }
}

async function main(): Promise<void> {
  const options = parseOptions();
  const client = await createClient(options);

  EventSerializer.registerEvent("ProductCreated", ProductCreated as any);
  EventSerializer.registerEvent("ProductSold", ProductSold as any);
  EventSerializer.registerEvent("OrderPlaced", OrderPlaced as any);

  const factory = new RepositoryFactory(client);
  const productRepo = factory.createRepository(() => new ProductAggregate(), "Product");
  const orderRepo = factory.createRepository(() => new OrderAggregate(), "Order");

  try {
    console.log("📊 Projections Example: E-commerce Analytics");
    console.log("==========================================");

    // Create products
    const productIds = [];
    for (let i = 0; i < 3; i++) {
      const id = `product-${randomUUID()}`;
      productIds.push(id);
      const product = new ProductAggregate();
      product.create(id, `Product ${i + 1}`, 100 + i * 50, 50);
      await productRepo.save(product);
      console.log(`📦 Created Product ${i + 1}`);
    }

    // Create orders and sales
    for (let i = 0; i < 5; i++) {
      const orderId = `order-${randomUUID()}`;
      const order = new OrderAggregate();
      order.place(orderId, `customer-${i}`, 150);
      await orderRepo.save(order);

      // Sell products
      const productId = productIds[i % productIds.length];
      const product = await productRepo.load(productId);
      if (product) {
        product.sell(2, orderId);
        await productRepo.save(product);
      }
      console.log(`🛒 Created order ${i + 1} and sold products`);
    }

    // Build projections
    console.log("\n🔄 Building projections from events...");
    const salesProjection = new SalesProjection();
    const catalogProjection = new CatalogProjection();

    // Process all events
    const allEvents = [];
    for (const id of productIds) {
      const events = await client.readEvents(`Product-${id}`);
      allEvents.push(...events);
    }
    for (let i = 0; i < 5; i++) {
      const events = await client.readEvents(`Order-order-${i}`);
      allEvents.push(...events);
    }

    allEvents.forEach(event => {
      salesProjection.processEvent(event);
      catalogProjection.processEvent(event);
    });

    // Query projections
    console.log("\n📊 PROJECTION RESULTS:");
    const salesReport = salesProjection.getReport();
    console.log(`💰 Sales: ${salesReport.totalOrders} orders, $${salesReport.totalRevenue} revenue`);
    
    const catalog = catalogProjection.getCatalog();
    console.log(`📦 Products:`);
    catalog.products.forEach(p => {
      console.log(`   ${p.name}: $${p.price}, Stock: ${p.stock}, Sold: ${p.totalSold}`);
    });

    console.log("\n🎉 Projections example completed!");
    console.log("💡 Demonstrates: Event-driven projections, multiple views, analytics");

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
