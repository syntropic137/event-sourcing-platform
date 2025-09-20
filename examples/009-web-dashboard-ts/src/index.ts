import { randomUUID } from "crypto";
import express from "express";
import path from "path";

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
  onPlaced() {}
}

// Dashboard Data
interface DashboardData {
  products: Array<{ id: string; name: string; price: number; stock: number; sold: number; revenue: number }>;
  sales: { totalOrders: number; totalRevenue: number; averageOrderValue: number };
  recentActivity: Array<{ timestamp: string; event: string; details: string }>;
  systemHealth: { status: string; uptime: number; eventsProcessed: number };
}

class DashboardProjection {
  private products = new Map<string, any>();
  private orders = new Map<string, any>();
  private events: any[] = [];
  private startTime = Date.now();

  processEvent(envelope: any) {
    const event = envelope.event;
    const metadata = envelope.metadata;
    const productId = metadata.aggregateId;

    this.events.push({
      timestamp: new Date(metadata.timestamp).toISOString(),
      event: event.eventType,
      details: this.getEventDetails(event, metadata)
    });

    switch (event.eventType) {
      case "ProductCreated":
        this.products.set(productId, {
          id: productId, name: event.name, price: event.price, stock: event.stock, sold: 0, revenue: 0
        });
        break;
      case "ProductSold":
        const product = this.products.get(productId);
        if (product) {
          product.stock -= event.quantity;
          product.sold += event.quantity;
          product.revenue += event.quantity * event.unitPrice;
        }
        break;
      case "OrderPlaced":
        this.orders.set(event.orderId, {
          id: event.orderId, customerId: event.customerId, amount: event.totalAmount
        });
        break;
    }
  }

  private getEventDetails(event: any, metadata: any): string {
    switch (event.eventType) {
      case "ProductCreated":
        return `Created ${event.name} with ${event.stock} units at $${event.price}`;
      case "ProductSold":
        return `Sold ${event.quantity} units for $${(event.quantity * event.unitPrice).toFixed(2)}`;
      case "OrderPlaced":
        return `Order ${event.orderId} placed for $${event.totalAmount.toFixed(2)}`;
      default:
        return JSON.stringify(event);
    }
  }

  clearData(): void {
    this.products.clear();
    this.orders.clear();
    this.events = [];
  }

  getDashboardData(): DashboardData {
    const products = Array.from(this.products.values());
    const orders = Array.from(this.orders.values());
    
    const totalRevenue = orders.reduce((sum, order) => sum + order.amount, 0);
    const averageOrderValue = orders.length > 0 ? totalRevenue / orders.length : 0;

    return {
      products,
      sales: {
        totalOrders: orders.length,
        totalRevenue,
        averageOrderValue
      },
      recentActivity: this.events.slice(-10).reverse(),
      systemHealth: {
        status: "healthy",
        uptime: Math.floor((Date.now() - this.startTime) / 1000),
        eventsProcessed: this.events.length
      }
    };
  }
}

async function createDashboardHTML(): Promise<string> {
  return `
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Event Sourcing Dashboard</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body { font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif; background: #f5f5f5; }
        .header { background: #2c3e50; color: white; padding: 1rem; text-align: center; }
        .container { max-width: 1200px; margin: 0 auto; padding: 2rem; }
        .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); gap: 1.5rem; }
        .card { background: white; border-radius: 8px; padding: 1.5rem; box-shadow: 0 2px 10px rgba(0,0,0,0.1); }
        .card h3 { color: #2c3e50; margin-bottom: 1rem; border-bottom: 2px solid #3498db; padding-bottom: 0.5rem; }
        .metric { display: flex; justify-content: space-between; margin: 0.5rem 0; }
        .metric-value { font-weight: bold; color: #27ae60; }
        .product-item { display: flex; justify-content: space-between; align-items: center; padding: 0.5rem; border-bottom: 1px solid #eee; }
        .product-name { font-weight: bold; }
        .product-stats { font-size: 0.9em; color: #666; }
        .activity-item { padding: 0.5rem; border-left: 3px solid #3498db; margin: 0.5rem 0; background: #f8f9fa; }
        .activity-time { font-size: 0.8em; color: #666; }
        .status-healthy { color: #27ae60; font-weight: bold; }
        .refresh-btn { background: #3498db; color: white; border: none; padding: 0.5rem 1rem; border-radius: 4px; cursor: pointer; }
        .refresh-btn:hover { background: #2980b9; }
        .auto-refresh { margin: 1rem 0; text-align: center; }
    </style>
</head>
<body>
    <div class="header">
        <h1>🚀 Event Sourcing Platform Dashboard</h1>
        <p>Real-time view of your event-sourced system</p>
    </div>
    
    <div class="container">
        <div class="auto-refresh">
            <button class="refresh-btn" onclick="refreshData()">🔄 Refresh Data</button>
            <button class="refresh-btn" onclick="generateData()">🎲 Generate Sample Data</button>
            <label><input type="checkbox" id="autoRefresh" checked> Auto-refresh every 5s</label>
        </div>
        
        <div class="grid">
            <div class="card">
                <h3>📊 Sales Metrics</h3>
                <div id="salesMetrics">Loading...</div>
            </div>
            
            <div class="card">
                <h3>🏥 System Health</h3>
                <div id="systemHealth">Loading...</div>
            </div>
            
            <div class="card">
                <h3>📦 Product Inventory</h3>
                <div id="productInventory">Loading...</div>
            </div>
            
            <div class="card">
                <h3>📈 Recent Activity</h3>
                <div id="recentActivity">Loading...</div>
            </div>
        </div>
    </div>

    <script>
        async function fetchDashboardData() {
            try {
                const response = await fetch('/api/dashboard');
                return await response.json();
            } catch (error) {
                console.error('Failed to fetch dashboard data:', error);
                return null;
            }
        }

        async function generateData() {
            try {
                const response = await fetch('/api/generate-data', { method: 'POST' });
                if (response.ok) {
                    alert('Sample data generated successfully!');
                    refreshData();
                }
            } catch (error) {
                alert('Failed to generate data: ' + error.message);
            }
        }

        function updateSalesMetrics(data) {
            const html = \`
                <div class="metric">
                    <span>Total Orders:</span>
                    <span class="metric-value">\${data.sales.totalOrders}</span>
                </div>
                <div class="metric">
                    <span>Total Revenue:</span>
                    <span class="metric-value">$\${data.sales.totalRevenue.toFixed(2)}</span>
                </div>
                <div class="metric">
                    <span>Average Order Value:</span>
                    <span class="metric-value">$\${data.sales.averageOrderValue.toFixed(2)}</span>
                </div>
            \`;
            document.getElementById('salesMetrics').innerHTML = html;
        }

        function updateSystemHealth(data) {
            const html = \`
                <div class="metric">
                    <span>Status:</span>
                    <span class="status-healthy">\${data.systemHealth.status.toUpperCase()}</span>
                </div>
                <div class="metric">
                    <span>Uptime:</span>
                    <span class="metric-value">\${data.systemHealth.uptime}s</span>
                </div>
                <div class="metric">
                    <span>Events Processed:</span>
                    <span class="metric-value">\${data.systemHealth.eventsProcessed}</span>
                </div>
            \`;
            document.getElementById('systemHealth').innerHTML = html;
        }

        function updateProductInventory(data) {
            const html = data.products.map(product => \`
                <div class="product-item">
                    <div>
                        <div class="product-name">\${product.name}</div>
                        <div class="product-stats">$\${product.price} • Stock: \${product.stock}</div>
                    </div>
                    <div class="product-stats">
                        Sold: \${product.sold} • Revenue: $\${product.revenue.toFixed(2)}
                    </div>
                </div>
            \`).join('');
            document.getElementById('productInventory').innerHTML = html || '<p>No products found</p>';
        }

        function updateRecentActivity(data) {
            const html = data.recentActivity.map(activity => \`
                <div class="activity-item">
                    <div><strong>\${activity.event}</strong></div>
                    <div>\${activity.details}</div>
                    <div class="activity-time">\${new Date(activity.timestamp).toLocaleString()}</div>
                </div>
            \`).join('');
            document.getElementById('recentActivity').innerHTML = html || '<p>No recent activity</p>';
        }

        async function refreshData() {
            const data = await fetchDashboardData();
            if (data) {
                updateSalesMetrics(data);
                updateSystemHealth(data);
                updateProductInventory(data);
                updateRecentActivity(data);
            }
        }

        // Auto-refresh functionality
        let autoRefreshInterval;
        function toggleAutoRefresh() {
            const checkbox = document.getElementById('autoRefresh');
            if (checkbox.checked) {
                autoRefreshInterval = setInterval(refreshData, 5000);
            } else {
                clearInterval(autoRefreshInterval);
            }
        }

        // Initialize
        document.addEventListener('DOMContentLoaded', () => {
            refreshData();
            toggleAutoRefresh();
            document.getElementById('autoRefresh').addEventListener('change', toggleAutoRefresh);
        });
    </script>
</body>
</html>
  `;
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
  const dashboard = new DashboardProjection();

  // Create Express app
  const app = express();
  const port = 3000;

  // Dashboard HTML route
  app.get('/', async (req, res) => {
    const html = await createDashboardHTML();
    res.send(html);
  });

  // API route for dashboard data
  app.get('/api/dashboard', async (req, res) => {
    try {
      res.json(dashboard.getDashboardData());
    } catch (error) {
      console.error('Dashboard API error:', error);
      res.status(500).json({ error: 'Failed to fetch dashboard data' });
    }
  });

  // API route to trigger sample data generation
  app.post('/api/generate-data', async (req, res) => {
    try {
      console.log("🎲 Generating sample data...");
      
      // Create some products
      const productNames = ["Laptop", "Mouse", "Keyboard", "Monitor", "Headphones"];
      const productIds = [];
      
      for (let i = 0; i < productNames.length; i++) {
        const productId = `product-${randomUUID()}`;
        productIds.push(productId);
        
        const product = new ProductAggregate();
        product.create(productId, productNames[i], 100 + i * 50, 20 + i * 10);
        await productRepo.save(product);
      }

      // Create some orders and sales
      const orderIds = [];
      for (let i = 0; i < 10; i++) {
        const orderId = `order-${randomUUID()}`;
        orderIds.push(orderId);
        
        const order = new OrderAggregate();
        const amount = 50 + Math.random() * 200;
        order.place(orderId, `customer-${i}`, amount);
        await orderRepo.save(order);

        // Sell some products
        const productId = productIds[Math.floor(Math.random() * productIds.length)];
        const product = await productRepo.load(productId);
        if (product) {
          const quantity = Math.floor(Math.random() * 3) + 1;
          try {
            product.sell(quantity, orderId);
            await productRepo.save(product);
          } catch (error) {
            // Ignore insufficient stock errors for demo
          }
        }
      }

      // Rebuild dashboard with new data
      const allEvents = [];
      
      // Read product events
      for (const productId of productIds) {
        const events = await client.readEvents(`Product-${productId}`);
        allEvents.push(...events);
      }
      
      // Read order events
      for (const orderId of orderIds) {
        try {
          const events = await client.readEvents(`Order-${orderId}`);
          allEvents.push(...events);
        } catch (error) {
          // Stream might not exist, that's ok
        }
      }

      // Clear and rebuild dashboard projections
      dashboard.clearData();
      allEvents.forEach(event => dashboard.processEvent(event));

      res.json({ success: true, message: "Sample data generated successfully" });
    } catch (error) {
      console.error('Data generation error:', error);
      res.status(500).json({ error: 'Failed to generate sample data' });
    }
  });

  try {
    console.log("🌐 Live Web Dashboard for Event Sourcing");
    console.log("=======================================");

    // Start the web server
    const server = app.listen(port, () => {
      console.log(`\n🚀 Dashboard server running at http://localhost:${port}`);
      console.log("📊 Open your browser to see live projections!");
      console.log("🎲 Click 'Generate Sample Data' to create demo data");
      console.log("🔄 Dashboard auto-refreshes every 5 seconds");
      console.log("\n💡 This demonstrates:");
      console.log("   • Live web dashboard showing event sourcing projections");
      console.log("   • Real-time updates from event streams");
      console.log("   • Visual feedback of the event sourcing system");
      console.log("   • Making the 'opaque' system transparent and observable");
    });

    // Keep the process running
    process.on('SIGINT', () => {
      console.log("\n👋 Shutting down dashboard server...");
      server.close(() => {
        client.disconnect().then(() => {
          process.exit(0);
        });
      });
    });

    // Keep the main function running
    await new Promise(() => {}); // Run forever

  } catch (error) {
    console.error("❌ Dashboard failed:", error);
    await client.disconnect();
    process.exit(1);
  }
}

if (require.main === module) {
  main().catch((error) => {
    console.error("❌ Example failed", error);
    process.exitCode = 1;
  });
}
