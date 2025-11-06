# Examples Overview

**Production-ready examples demonstrating Hexagonal Event-Sourced VSA Architecture**

---

## 🚀 Quick Start

We have **3 carefully crafted examples** that demonstrate event-sourced hexagonal VSA architecture, progressing from simple to advanced:

| Example | Complexity | Description |
|---------|------------|-------------|
| **[002-simple-aggregate-ts](./002-simple-aggregate-ts/)** | ⭐ Simple | Perfect starting point - Order aggregate basics |
| **[004-cqrs-patterns-ts](./004-cqrs-patterns-ts/)** | ⭐⭐ Medium | CQRS with projections and read models |
| **[007-ecommerce-complete-ts](./007-ecommerce-complete-ts/)** | ⭐⭐⭐ Advanced | Multi-aggregate e-commerce platform |

### Why These Examples?

✅ **Hexagonal Architecture** - Clear separation: Domain / Infrastructure / Adapters  
✅ **Event Versioning** - All events use `@Event("...", "v1")` decorators  
✅ **VSA Validated** - Includes `vsa.yaml` for CLI validation  
✅ **Production Patterns** - Battle-tested architectural patterns  
✅ **Comprehensive Docs** - Each example has detailed README  
✅ **Well Tested** - Builds, runs, and demonstrates complete workflows  

---

## 🎓 Learning Path

### 1. Start Here: Simple Aggregate (30 minutes)
**[002-simple-aggregate-ts](./002-simple-aggregate-ts/)**

**Run it:**
```bash
cd examples/002-simple-aggregate-ts
npm install
npm run dev -- --memory
```

**You'll learn:**
- Hexagonal architecture basics
- Domain/Infrastructure separation
- `@Aggregate`, `@CommandHandler`, `@EventSourcingHandler` decorators
- Event versioning with `@Event("...", "v1")`
- CommandBus pattern
- Repository pattern

**Structure:**
```
src/
├── domain/              # 🔵 CORE (business logic)
│   ├── OrderAggregate.ts
│   ├── commands/        # 2 commands
│   └── events/          # 2 events with @Event decorators
├── infrastructure/      # 🟢 APPLICATION SERVICES
│   └── CommandBus.ts
├── slices/              # 🟡 ADAPTERS
│   ├── submit-order/
│   └── cancel-order/
└── main.ts
```

---

### 2. CQRS Patterns (45 minutes)
**[004-cqrs-patterns-ts](./004-cqrs-patterns-ts/)**

**Run it:**
```bash
cd examples/004-cqrs-patterns-ts
npm install
npm run dev -- --memory
```

**You'll learn:**
- CQRS pattern (write side vs read side)
- Projections building read models from events
- QueryBus alongside CommandBus
- Denormalized views for optimized queries
- Vertical slices for queries

**Demonstrates:**
- 4 Commands (write): Open, Deposit, Withdraw, Close
- 3 Queries (read): GetAccountSummary, GetTransactionHistory, GetAccountsByCustomer
- 2 Projections: AccountSummaryProjection, TransactionHistoryProjection

**Pattern:**
```
Write Side:              Read Side:
Commands                 Queries
   ↓                        ↓
CommandBus               QueryBus
   ↓                        ↓
Aggregate                Projection
   ↓                        ↑
Events ──────────────────────┘
```

---

### 3. Multi-Aggregate Systems (60 minutes)
**[007-ecommerce-complete-ts](./007-ecommerce-complete-ts/)**

**Run it:**
```bash
cd examples/007-ecommerce-complete-ts
npm install
npm run dev -- --memory
```

**You'll learn:**
- Multiple aggregates in one system
- State machines (Order: DRAFT → CONFIRMED → SHIPPED)
- Cross-aggregate coordination
- Complete business workflow end-to-end
- Stock management with validation

**Demonstrates:**
- 3 Aggregates: Product, Order, Customer
- 11 Commands in `domain/commands/`
- 11 Events with `@Event` decorators
- CommandBus routing to correct aggregate

**Workflow:**
```
1. Register Customer → CustomerAggregate
2. Create Product    → ProductAggregate
3. Add Stock         → ProductAggregate
4. Create Order      → OrderAggregate (DRAFT)
5. Add Items         → OrderAggregate
6. Confirm Order     → OrderAggregate (CONFIRMED)
7. Remove Stock      → ProductAggregate
8. Ship Order        → OrderAggregate (SHIPPED)
```

---

## 🏗️ Architecture Overview

All examples follow **Hexagonal Event-Sourced VSA** architecture:

```
┌─────────────────────────────────────────────────────────┐
│                   HEXAGONAL ARCHITECTURE                 │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  🟡 ADAPTERS (Outside)                                  │
│  └─ Thin controllers (REST, CLI, gRPC)                 │
│           ↓                                              │
│  🟢 INFRASTRUCTURE (Application Services)               │
│  ├─ CommandBus → Routes to Aggregates                  │
│  └─ QueryBus   → Routes to Projections                 │
│           ↓                                              │
│  🔵 DOMAIN (Core - No Dependencies)                    │
│  ├─ Aggregates → Business Logic                        │
│  ├─ Commands   → Intent                                │
│  ├─ Events     → Facts (@Event decorators)             │
│  └─ Queries    → Read Requests                         │
│                                                          │
└─────────────────────────────────────────────────────────┘
```

**Key Rules:**
- ✅ Domain has NO outward dependencies
- ✅ All dependencies point INWARD
- ✅ Adapters translate protocols to commands
- ✅ Infrastructure coordinates between layers

---

## 🎯 Key Patterns

### Command Pattern
```typescript
// domain/commands/SubmitOrderCommand.ts
export class SubmitOrderCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly orderId: string,
    public readonly customerId: string
  ) {}
}

// domain/OrderAggregate.ts
@Aggregate("Order")
export class OrderAggregate extends AggregateRoot<OrderEvent> {
  @CommandHandler("SubmitOrderCommand")
  submit(command: SubmitOrderCommand): void {
    if (this.status !== OrderStatus.New) {
      throw new Error("Cannot submit order");
    }
    this.apply(new OrderSubmittedEvent(...));
  }
  
  @EventSourcingHandler("OrderSubmitted")
  private onSubmitted(event: OrderSubmittedEvent): void {
    this.status = OrderStatus.Submitted;
  }
}
```

### Event Versioning
```typescript
// domain/events/OrderSubmittedEvent.ts
import { BaseDomainEvent, Event } from "@event-sourcing-platform/typescript";

@Event("OrderSubmitted", "v1")  // ← Version decorator!
export class OrderSubmittedEvent extends BaseDomainEvent {
  readonly eventType = "OrderSubmitted" as const;
  readonly schemaVersion = 1 as const;
  // ...
}
```

### CommandBus (Infrastructure)
```typescript
// infrastructure/CommandBus.ts
export class CommandBus {
  async send(command: SupportedCommands): Promise<void> {
    const repository = this.repositoryFactory.createRepository(
      () => new OrderAggregate(),
      "Order"
    );
    let aggregate = await repository.load(command.aggregateId);
    if (!aggregate) aggregate = new OrderAggregate();
    
    (aggregate as any).handleCommand(command);
    await repository.save(aggregate);
  }
}
```

---

## 📦 Running Examples

### Individual Example
```bash
cd examples/002-simple-aggregate-ts
npm install
npm run dev -- --memory
```

### Validate Architecture
```bash
cd examples/002-simple-aggregate-ts
npm run validate  # (requires vsa CLI)
```

### All Examples
```bash
# From workspace root
make examples-run
```

---

## 📖 Documentation

### Architecture Decision Records
- [ADR-004: Command Handlers in Aggregates](../docs/adrs/ADR-004-command-handlers-in-aggregates.md)
- [ADR-005: Hexagonal Architecture](../docs/adrs/ADR-005-hexagonal-architecture-event-sourcing.md)
- [ADR-006: Domain Organization](../docs/adrs/ADR-006-domain-organization-pattern.md)
- [ADR-007: Event Versioning](../docs/adrs/ADR-007-event-versioning-upcasters.md)
- [ADR-008: Vertical Slices](../docs/adrs/ADR-008-vertical-slices-hexagonal-adapters.md)
- [ADR-009: CQRS Pattern](../docs/adrs/ADR-009-cqrs-pattern-implementation.md)
- [ADR-010: Decorator Patterns](../docs/adrs/ADR-010-decorator-patterns-framework.md)

### Guides
- [Hexagonal VSA Quick Start](../docs/HEXAGONAL-VSA-QUICK-START.md)
- [VSA Tool Demonstration](../VSA-TOOL-DEMONSTRATION.md)
- [ADR Index](../docs/adrs/ADR-INDEX.md)

---

## 🤝 Contributing

When adding new examples:

1. **Follow Hexagonal VSA Pattern** - Use 002, 004, or 007 as templates
2. **Include `vsa.yaml`** - For architecture validation
3. **Use Decorators** - `@Event`, `@Command`, `@Query`, `@Aggregate`
4. **Comprehensive README** - Explain what it demonstrates
5. **Test Thoroughly** - Ensure it builds and runs
6. **Update This File** - Add to the catalogue above

---

## 🎯 Next Steps

1. **Start with 002** - Learn hexagonal VSA basics
2. **Progress to 004** - Understand CQRS and projections
3. **Master with 007** - See multi-aggregate architecture
4. **Read the ADRs** - Understand architectural decisions
5. **Build Your Own** - Use these as templates

---

**Last Updated:** November 6, 2025  
**Architecture:** Hexagonal Event-Sourced VSA (v2.0)  
**Examples:** 3 production-ready patterns
