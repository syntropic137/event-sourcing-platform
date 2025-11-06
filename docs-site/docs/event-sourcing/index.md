# Event Sourcing Platform

Welcome to the comprehensive Event Sourcing Platform! This platform provides everything you need to build production-ready event-sourced applications.

## 🎯 What is Event Sourcing?

Event Sourcing is a powerful architectural pattern where you store the **sequence of events** that led to the current state, rather than just storing the current state itself. Think of it like a **bank statement** - instead of just knowing your current balance, you have a complete history of every transaction.

### Key Benefits
- **🔍 Complete Audit Trail** - Every change is recorded with full context
- **⏰ Time Travel** - Reconstruct state at any point in time  
- **🔄 Replay & Recovery** - Rebuild systems from events
- **📊 Analytics & Insights** - Rich data for business intelligence
- **🚀 Scalability** - Natural fit for distributed systems

## 🏗️ Platform Architecture

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Applications  │    │   Event Store   │    │   Projections   │
│                 │    │                 │    │                 │
│  • Aggregates   │───▶│  • Events       │───▶│  • Read Models  │
│  • Commands     │    │  • Streams      │    │  • Analytics    │
│  • Queries      │    │  • Snapshots    │    │  • Dashboards   │
└─────────────────┘    └─────────────────┘    └─────────────────┘
```

## 🎓 Learning Journey

Our platform includes **9 progressive examples** that take you from beginner to expert:

### 🏗️ Foundation (Examples 001-003)
Learn the core building blocks of event sourcing

### 🎨 Patterns (Examples 004-006)  
Master advanced architectural patterns like CQRS and projections

### 🏢 Complete Systems (Examples 007-009)
Build production-ready applications with monitoring and visualization

## 🛠️ Core Components

### TypeScript SDK
- **Aggregates** - Business logic containers
- **Events** - Immutable facts about what happened
- **Repositories** - Persistence abstractions
- **Projections** - Read model builders
- **Event Bus** - Cross-aggregate communication

### Event Store
- **gRPC API** - High-performance event persistence
- **PostgreSQL Backend** - Reliable, ACID-compliant storage
- **Optimistic Concurrency** - Safe concurrent modifications
- **Subscriptions** - Real-time event streaming

### Development Tools
- **Docker Infrastructure** - PostgreSQL + Redis containers
- **Live Dashboard** - Visual event sourcing system
- **Example Applications** - Progressive learning path
- **Comprehensive Testing** - Unit, integration, and end-to-end tests

## 🚀 Quick Start

```bash
# 1. Start infrastructure
make dev-start

# 2. Run an example
pnpm --filter "./examples/001-basic-store-ts" run start

# 3. View the live dashboard
pnpm --filter "./examples/009-web-dashboard-ts" run start
```

## 📚 Key Concepts

### Ubiquitous Language

**Event** - An immutable fact about something that happened in the past
```typescript
interface OrderPlaced {
  orderId: string;
  customerId: string;
  items: LineItem[];
  totalAmount: number;
}
```

**Aggregate** - A consistency boundary that processes commands and emits events
```typescript
class OrderAggregate extends Aggregate {
  place(orderId: string, customerId: string, items: LineItem[]) {
    // Business logic validation
    this.raiseEvent(new OrderPlaced(orderId, customerId, items));
  }
}
```

**Command** - An intention to change state (use classes with `aggregateId`)
```typescript
class PlaceOrderCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly customerId: string,
    public readonly items: LineItem[]
  ) {}
}
```

**Projection** - A read model built from events
```typescript
class OrderSummaryProjection {
  handleEvent(event: DomainEvent) {
    if (event instanceof OrderPlaced) {
      this.updateOrderSummary(event);
    }
  }
}
```

## 🎯 When to Use Event Sourcing

### ✅ Great For:
- **Audit Requirements** - Financial, healthcare, legal systems
- **Complex Business Logic** - Multi-step workflows
- **Analytics & Reporting** - Rich historical data
- **Distributed Systems** - Natural event-driven architecture
- **Debugging & Troubleshooting** - Complete system history

### ⚠️ Consider Carefully:
- **Simple CRUD Applications** - May be overkill
- **High-Frequency Updates** - Event volume can be challenging
- **Team Experience** - Requires understanding of the pattern
- **Query Complexity** - Read models need careful design

## 🔍 Explore Further

- **[Examples](./examples/)** - Hands-on learning with 9 progressive examples
- **Event Store** - Deep dive into persistence layer
- **Development** - Setup and workflow guides
- **Concepts** - Detailed architectural concepts

## 📚 Resources & Inspiration

This platform draws inspiration from leading event sourcing practitioners and resources:

- **[Understanding Event Sourcing](https://leanpub.com/eventsourcing)** by Martin Dilger - A comprehensive book combining Event Modeling and Event Sourcing to plan and build software systems. Features Kotlin/Spring/Axon sample code. [GitHub repository](https://github.com/dilgerma/eventsourcing-book).
- **[Event Modeling](https://eventmodeling.org/)** - The original Event Modeling methodology article
- **[Event Sourcing Basics](https://martinfowler.com/eaaDev/EventSourcing.html)** by Martin Fowler - Foundational concepts
- **[CQRS](https://martinfowler.com/bliki/CQRS.html)** - Command Query Responsibility Segregation pattern

Ready to start your event sourcing journey? Begin with our Examples section!
