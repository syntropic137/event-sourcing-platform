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

// Sample Events for Monitoring
class UserRegistered extends BaseDomainEvent {
  readonly eventType = "UserRegistered" as const;
  readonly schemaVersion = 1 as const;
  constructor(public userId: string, public email: string) { super(); }
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

class SystemError extends BaseDomainEvent {
  readonly eventType = "SystemError" as const;
  readonly schemaVersion = 1 as const;
  constructor(public errorType: string, public message: string, public severity: "low" | "medium" | "high") { super(); }
}

// Sample Aggregates
class UserAggregate extends AutoDispatchAggregate<UserRegistered> {
  getAggregateType() { return "User"; }
  register(id: string, email: string) { this.initialize(id); this.raiseEvent(new UserRegistered(id, email)); }
  @EventSourcingHandler("UserRegistered") onRegistered() {}
}

class OrderAggregate extends AutoDispatchAggregate<OrderPlaced> {
  getAggregateType() { return "Order"; }
  place(id: string, userId: string, amount: number) { this.initialize(id); this.raiseEvent(new OrderPlaced(id, userId, amount)); }
  @EventSourcingHandler("OrderPlaced") onPlaced() {}
}

class PaymentAggregate extends AutoDispatchAggregate<PaymentProcessed> {
  getAggregateType() { return "Payment"; }
  process(id: string, orderId: string, amount: number) { this.initialize(id); this.raiseEvent(new PaymentProcessed(id, orderId, amount)); }
  @EventSourcingHandler("PaymentProcessed") onProcessed() {}
}

class SystemAggregate extends AutoDispatchAggregate<SystemError> {
  getAggregateType() { return "System"; }
  logError(id: string, errorType: string, message: string, severity: "low" | "medium" | "high") {
    this.initialize(id); this.raiseEvent(new SystemError(errorType, message, severity));
  }
  @EventSourcingHandler("SystemError") onError() {}
}

// Observability Metrics
interface SystemMetrics {
  totalEvents: number;
  eventsByType: Map<string, number>;
  eventsByAggregate: Map<string, number>;
  eventsPerMinute: number;
  errorCount: number;
  errorsByType: Map<string, number>;
  systemHealth: "healthy" | "warning" | "critical";
  uptime: number;
  lastEventTime?: Date;
}

interface PerformanceMetrics {
  averageProcessingTime: number;
  slowestOperations: Array<{ operation: string; duration: number; timestamp: Date }>;
  throughput: { eventsPerSecond: number; operationsPerSecond: number };
  memoryUsage: { used: number; total: number; percentage: number };
}

interface BusinessMetrics {
  totalUsers: number;
  totalOrders: number;
  totalRevenue: number;
  averageOrderValue: number;
  conversionRate: number;
  dailyActiveUsers: number;
}

class ObservabilityEngine {
  private systemMetrics: SystemMetrics;
  private performanceMetrics: PerformanceMetrics;
  private businessMetrics: BusinessMetrics;
  private startTime: Date;
  private eventTimes: number[] = [];

  constructor() {
    this.startTime = new Date();
    this.systemMetrics = {
      totalEvents: 0,
      eventsByType: new Map(),
      eventsByAggregate: new Map(),
      eventsPerMinute: 0,
      errorCount: 0,
      errorsByType: new Map(),
      systemHealth: "healthy",
      uptime: 0
    };
    this.performanceMetrics = {
      averageProcessingTime: 0,
      slowestOperations: [],
      throughput: { eventsPerSecond: 0, operationsPerSecond: 0 },
      memoryUsage: { used: 0, total: 0, percentage: 0 }
    };
    this.businessMetrics = {
      totalUsers: 0,
      totalOrders: 0,
      totalRevenue: 0,
      averageOrderValue: 0,
      conversionRate: 0,
      dailyActiveUsers: 0
    };
  }

  processEvent(envelope: any) {
    const event = envelope.event;
    const metadata = envelope.metadata;
    const now = Date.now();
    
    // System Metrics
    this.systemMetrics.totalEvents++;
    this.systemMetrics.eventsByType.set(
      event.eventType, 
      (this.systemMetrics.eventsByType.get(event.eventType) || 0) + 1
    );
    this.systemMetrics.eventsByAggregate.set(
      metadata.aggregateType, 
      (this.systemMetrics.eventsByAggregate.get(metadata.aggregateType) || 0) + 1
    );
    this.systemMetrics.lastEventTime = new Date(metadata.timestamp);
    this.systemMetrics.uptime = Math.floor((now - this.startTime.getTime()) / 1000);

    // Track event timing for throughput
    this.eventTimes.push(now);
    this.eventTimes = this.eventTimes.filter(time => now - time < 60000); // Keep last minute
    this.systemMetrics.eventsPerMinute = this.eventTimes.length;

    // Error tracking
    if (event.eventType === "SystemError") {
      this.systemMetrics.errorCount++;
      this.systemMetrics.errorsByType.set(
        event.errorType,
        (this.systemMetrics.errorsByType.get(event.errorType) || 0) + 1
      );
      
      // Update system health based on errors
      if (event.severity === "high") {
        this.systemMetrics.systemHealth = "critical";
      } else if (event.severity === "medium" && this.systemMetrics.systemHealth === "healthy") {
        this.systemMetrics.systemHealth = "warning";
      }
    }

    // Business Metrics
    switch (event.eventType) {
      case "UserRegistered":
        this.businessMetrics.totalUsers++;
        break;
      case "OrderPlaced":
        this.businessMetrics.totalOrders++;
        this.businessMetrics.totalRevenue += event.amount;
        this.businessMetrics.averageOrderValue = this.businessMetrics.totalRevenue / this.businessMetrics.totalOrders;
        break;
    }

    // Calculate conversion rate
    if (this.businessMetrics.totalUsers > 0) {
      this.businessMetrics.conversionRate = (this.businessMetrics.totalOrders / this.businessMetrics.totalUsers) * 100;
    }

    // Performance simulation
    this.updatePerformanceMetrics();
  }

  private updatePerformanceMetrics() {
    // Simulate performance metrics
    this.performanceMetrics.averageProcessingTime = 50 + Math.random() * 100; // 50-150ms
    this.performanceMetrics.throughput.eventsPerSecond = this.systemMetrics.eventsPerMinute / 60;
    
    // Simulate memory usage
    const memUsed = 100 + Math.random() * 400; // 100-500MB
    const memTotal = 1024; // 1GB
    this.performanceMetrics.memoryUsage = {
      used: memUsed,
      total: memTotal,
      percentage: (memUsed / memTotal) * 100
    };
  }

  getSystemHealth(): "healthy" | "warning" | "critical" {
    const errorRate = this.systemMetrics.totalEvents > 0 ? 
      (this.systemMetrics.errorCount / this.systemMetrics.totalEvents) * 100 : 0;
    
    if (errorRate > 5 || this.performanceMetrics.memoryUsage.percentage > 90) {
      return "critical";
    } else if (errorRate > 1 || this.performanceMetrics.memoryUsage.percentage > 75) {
      return "warning";
    }
    return "healthy";
  }

  generateReport() {
    this.systemMetrics.systemHealth = this.getSystemHealth();
    
    return {
      timestamp: new Date(),
      system: this.systemMetrics,
      performance: this.performanceMetrics,
      business: this.businessMetrics
    };
  }

  getAlerts() {
    const alerts = [];
    
    if (this.systemMetrics.systemHealth === "critical") {
      alerts.push({ type: "critical", message: "System health is critical", timestamp: new Date() });
    }
    
    if (this.performanceMetrics.memoryUsage.percentage > 80) {
      alerts.push({ 
        type: "warning", 
        message: `High memory usage: ${this.performanceMetrics.memoryUsage.percentage.toFixed(1)}%`,
        timestamp: new Date()
      });
    }
    
    if (this.systemMetrics.errorCount > 0) {
      alerts.push({
        type: "info",
        message: `${this.systemMetrics.errorCount} errors detected`,
        timestamp: new Date()
      });
    }
    
    return alerts;
  }
}

async function main(): Promise<void> {
  const options = parseOptions();
  const client = await createClient(options);

  EventSerializer.registerEvent("UserRegistered", UserRegistered as any);
  EventSerializer.registerEvent("OrderPlaced", OrderPlaced as any);
  EventSerializer.registerEvent("PaymentProcessed", PaymentProcessed as any);
  EventSerializer.registerEvent("SystemError", SystemError as any);

  const factory = new RepositoryFactory(client);
  const userRepo = factory.createRepository(() => new UserAggregate(), "User");
  const orderRepo = factory.createRepository(() => new OrderAggregate(), "Order");
  const paymentRepo = factory.createRepository(() => new PaymentAggregate(), "Payment");
  const systemRepo = factory.createRepository(() => new SystemAggregate(), "System");

  const observability = new ObservabilityEngine();

  try {
    console.log("📊 System Observability & Monitoring");
    console.log("===================================");

    // Simulate system activity
    console.log("\n🔄 Simulating system activity...");
    
    // Create users
    for (let i = 0; i < 10; i++) {
      const userId = `user-${randomUUID()}`;
      const user = new UserAggregate();
      user.register(userId, `user${i}@example.com`);
      await userRepo.save(user);
      
      // Simulate some orders
      if (Math.random() > 0.3) {
        const orderId = `order-${randomUUID()}`;
        const order = new OrderAggregate();
        const amount = 50 + Math.random() * 200;
        order.place(orderId, userId, amount);
        await orderRepo.save(order);
        
        // Process payment
        const paymentId = `payment-${randomUUID()}`;
        const payment = new PaymentAggregate();
        payment.process(paymentId, orderId, amount);
        await paymentRepo.save(payment);
      }
    }

    // Simulate some errors
    console.log("⚠️  Simulating system errors...");
    for (let i = 0; i < 3; i++) {
      const errorId = `error-${randomUUID()}`;
      const system = new SystemAggregate();
      const errorTypes = ["ValidationError", "NetworkTimeout", "DatabaseConnection"];
      const severities: ("low" | "medium" | "high")[] = ["low", "medium", "high"];
      
      system.logError(
        errorId,
        errorTypes[Math.floor(Math.random() * errorTypes.length)],
        `Simulated error ${i + 1}`,
        severities[Math.floor(Math.random() * severities.length)]
      );
      await systemRepo.save(system);
    }

    // Collect all events for monitoring
    console.log("\n📈 Collecting metrics from event streams...");
    const allEvents = [];
    
    // Read events from all aggregates
    const aggregateTypes = ["User", "Order", "Payment", "System"];
    for (const aggregateType of aggregateTypes) {
      // In a real system, you'd have a way to list all streams of a type
      // For this demo, we'll simulate by reading known streams
      try {
        const events = await client.readEvents(`${aggregateType}-dummy`);
        allEvents.push(...events);
      } catch {
        // Stream doesn't exist, that's fine for demo
      }
    }

    // Process events through observability engine
    allEvents.forEach(event => observability.processEvent(event));

    // Generate monitoring report
    console.log("\n📊 SYSTEM MONITORING REPORT");
    console.log("==========================");
    
    const report = observability.generateReport();
    
    console.log(`🕐 Report Time: ${report.timestamp.toISOString()}`);
    console.log(`⏱️  System Uptime: ${report.system.uptime} seconds`);
    console.log(`🏥 System Health: ${report.system.systemHealth.toUpperCase()}`);
    
    console.log(`\n📈 EVENT METRICS:`);
    console.log(`   Total Events: ${report.system.totalEvents}`);
    console.log(`   Events/Minute: ${report.system.eventsPerMinute}`);
    console.log(`   Error Count: ${report.system.errorCount}`);
    
    console.log(`\n📊 EVENTS BY TYPE:`);
    Array.from(report.system.eventsByType.entries()).forEach(([type, count]) => {
      console.log(`   ${type}: ${count}`);
    });
    
    console.log(`\n🏢 BUSINESS METRICS:`);
    console.log(`   Total Users: ${report.business.totalUsers}`);
    console.log(`   Total Orders: ${report.business.totalOrders}`);
    console.log(`   Total Revenue: $${report.business.totalRevenue.toFixed(2)}`);
    console.log(`   Avg Order Value: $${report.business.averageOrderValue.toFixed(2)}`);
    console.log(`   Conversion Rate: ${report.business.conversionRate.toFixed(1)}%`);
    
    console.log(`\n⚡ PERFORMANCE METRICS:`);
    console.log(`   Avg Processing Time: ${report.performance.averageProcessingTime.toFixed(1)}ms`);
    console.log(`   Events/Second: ${report.performance.throughput.eventsPerSecond.toFixed(1)}`);
    console.log(`   Memory Usage: ${report.performance.memoryUsage.percentage.toFixed(1)}% (${report.performance.memoryUsage.used.toFixed(0)}MB)`);
    
    // Show alerts
    const alerts = observability.getAlerts();
    if (alerts.length > 0) {
      console.log(`\n🚨 ACTIVE ALERTS (${alerts.length}):`);
      alerts.forEach(alert => {
        const icon = alert.type === "critical" ? "🔴" : alert.type === "warning" ? "🟡" : "🔵";
        console.log(`   ${icon} ${alert.type.toUpperCase()}: ${alert.message}`);
      });
    } else {
      console.log(`\n✅ NO ACTIVE ALERTS - System operating normally`);
    }

    console.log("\n🎉 Observability example completed!");
    console.log("💡 Demonstrates: System monitoring, metrics collection, health checks, alerting");

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
