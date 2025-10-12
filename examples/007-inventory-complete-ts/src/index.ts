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
  constructor(public productId: string, public name: string, public sku: string, public reorderLevel: number) { super(); }
}

class StockReceived extends BaseDomainEvent {
  readonly eventType = "StockReceived" as const;
  readonly schemaVersion = 1 as const;
  constructor(public quantity: number, public supplierId: string) { super(); }
}

class StockSold extends BaseDomainEvent {
  readonly eventType = "StockSold" as const;
  readonly schemaVersion = 1 as const;
  constructor(public quantity: number, public orderId: string) { super(); }
}

class ReorderTriggered extends BaseDomainEvent {
  readonly eventType = "ReorderTriggered" as const;
  readonly schemaVersion = 1 as const;
  constructor(public quantity: number, public supplierId: string) { super(); }
}

// Aggregates
class ProductAggregate extends AutoDispatchAggregate<ProductCreated | StockReceived | StockSold | ReorderTriggered> {
  private name = ""; private sku = ""; private currentStock = 0; private reorderLevel = 0;
  
  getAggregateType() { return "Product"; }
  
  create(id: string, name: string, sku: string, reorderLevel: number) {
    this.initialize(id);
    this.raiseEvent(new ProductCreated(id, name, sku, reorderLevel));
  }
  
  receiveStock(quantity: number, supplierId: string) {
    this.raiseEvent(new StockReceived(quantity, supplierId));
  }
  
  sellStock(quantity: number, orderId: string) {
    if (this.currentStock < quantity) throw new Error("Insufficient stock");
    this.raiseEvent(new StockSold(quantity, orderId));
  }
  
  triggerReorder(quantity: number, supplierId: string) {
    this.raiseEvent(new ReorderTriggered(quantity, supplierId));
  }

  @EventSourcingHandler("ProductCreated")
  onCreated(e: ProductCreated) { this.name = e.name; this.sku = e.sku; this.reorderLevel = e.reorderLevel; }
  
  @EventSourcingHandler("StockReceived")
  onStockReceived(e: StockReceived) { this.currentStock += e.quantity; }
  
  @EventSourcingHandler("StockSold")
  onStockSold(e: StockSold) { this.currentStock -= e.quantity; }
  
  @EventSourcingHandler("ReorderTriggered")
  onReorderTriggered() { /* tracking only */ }
  
  getCurrentStock() { return this.currentStock; }
  getReorderLevel() { return this.reorderLevel; }
  needsReorder() { return this.currentStock <= this.reorderLevel; }
  getName() { return this.name; }
  getSku() { return this.sku; }
}

// Projections
interface InventoryView {
  productId: string; name: string; sku: string; currentStock: number; reorderLevel: number;
  needsReorder: boolean; totalSold: number; totalReceived: number;
}

class InventoryProjection {
  private inventory = new Map<string, InventoryView>();
  
  processEvent(envelope: any) {
    const event = envelope.event;
    const productId = envelope.metadata.aggregateId;
    
    switch (event.eventType) {
      case "ProductCreated":
        this.inventory.set(productId, {
          productId, name: event.name, sku: event.sku, currentStock: 0, reorderLevel: event.reorderLevel,
          needsReorder: false, totalSold: 0, totalReceived: 0
        });
        break;
      case "StockReceived":
        const product = this.inventory.get(productId);
        if (product) {
          product.currentStock += event.quantity;
          product.totalReceived += event.quantity;
          product.needsReorder = product.currentStock <= product.reorderLevel;
        }
        break;
      case "StockSold":
        const soldProduct = this.inventory.get(productId);
        if (soldProduct) {
          soldProduct.currentStock -= event.quantity;
          soldProduct.totalSold += event.quantity;
          soldProduct.needsReorder = soldProduct.currentStock <= soldProduct.reorderLevel;
        }
        break;
    }
  }
  
  getAllProducts() { return Array.from(this.inventory.values()); }
  getProduct(id: string) { return this.inventory.get(id); }
  getLowStockProducts() { return Array.from(this.inventory.values()).filter(p => p.needsReorder); }
}

async function main(): Promise<void> {
  const options = parseOptions();
  const client = await createClient(options);

  EventSerializer.registerEvent("ProductCreated", ProductCreated as any);
  EventSerializer.registerEvent("StockReceived", StockReceived as any);
  EventSerializer.registerEvent("StockSold", StockSold as any);
  EventSerializer.registerEvent("ReorderTriggered", ReorderTriggered as any);

  const factory = new RepositoryFactory(client);
  const productRepo = factory.createRepository(() => new ProductAggregate(), "Product");
  const inventoryProjection = new InventoryProjection();

  try {
    console.log("📦 Complete Inventory Management System");
    console.log("=====================================");

    // Create products
    const products = [
      { name: "Laptop", sku: "LAP001", reorderLevel: 5 },
      { name: "Mouse", sku: "MOU001", reorderLevel: 20 },
      { name: "Keyboard", sku: "KEY001", reorderLevel: 10 }
    ];
    
    const productIds = [];
    for (const productData of products) {
      const productId = `product-${randomUUID()}`;
      productIds.push(productId);
      
      const product = new ProductAggregate();
      product.create(productId, productData.name, productData.sku, productData.reorderLevel);
      product.receiveStock(50, "supplier-main"); // Initial stock
      await productRepo.save(product);
      console.log(`📦 Created ${productData.name} with 50 units (reorder at ${productData.reorderLevel})`);
    }

    // Simulate sales
    console.log("\n🛒 Simulating sales activity...");
    for (let i = 0; i < 20; i++) {
      const productId = productIds[i % productIds.length];
      const product = await productRepo.load(productId);
      if (product && product.getCurrentStock() > 0) {
        const quantity = Math.min(Math.floor(Math.random() * 5) + 1, product.getCurrentStock());
        product.sellStock(quantity, `order-${i}`);
        await productRepo.save(product);
        
        // Check if reorder needed
        if (product.needsReorder()) {
          const reorderQty = product.getReorderLevel() * 3; // Order 3x reorder level
          product.triggerReorder(reorderQty, "supplier-main");
          product.receiveStock(reorderQty, "supplier-main"); // Simulate delivery
          await productRepo.save(product);
          console.log(`🔄 Auto-reordered ${reorderQty} units of ${product.getName()}`);
        }
      }
    }

    // Build projections
    console.log("\n📊 Building inventory projections...");
    const allEvents = [];
    for (const productId of productIds) {
      const events = await client.readEvents(`Product-${productId}`);
      allEvents.push(...events);
    }

    allEvents.forEach(event => inventoryProjection.processEvent(event));

    // Display results
    console.log("\n📋 FINAL INVENTORY STATUS:");
    const inventory = inventoryProjection.getAllProducts();
    inventory.forEach(item => {
      const status = item.needsReorder ? "⚠️  LOW" : "✅ OK";
      console.log(`   ${item.name} (${item.sku}): ${item.currentStock} units ${status}`);
      console.log(`      Sold: ${item.totalSold}, Received: ${item.totalReceived}, Reorder Level: ${item.reorderLevel}`);
    });

    const lowStock = inventoryProjection.getLowStockProducts();
    console.log(`\n🔔 LOW STOCK ALERTS: ${lowStock.length} products need attention`);
    lowStock.forEach(item => {
      console.log(`   ⚠️  ${item.name}: ${item.currentStock} units (reorder at ${item.reorderLevel})`);
    });

    console.log("\n🎉 Inventory management example completed!");
    console.log("💡 Demonstrates: Stock tracking, automatic reordering, inventory projections");

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
