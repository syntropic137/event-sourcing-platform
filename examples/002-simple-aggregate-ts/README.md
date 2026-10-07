# Example 002 — Simple Aggregate (TypeScript) ⭐ Beginner

**A beginner-friendly example demonstrating Hexagonal Event-Sourced VSA Architecture.**

This example showcases the foundational patterns for event-sourced systems:
- ✅ **Domain-Driven Design** - Aggregates with business logic
- ✅ **Event Sourcing** - Events as the source of truth
- ✅ **Hexagonal Architecture** - Clean separation of concerns
- ✅ **Vertical Slice Architecture** - Feature-based organization
- ✅ **CQRS** - Separate write (commands) and read (queries) models

## 🏗️ Architecture

This example follows the **Hexagonal Event-Sourced VSA** pattern as defined in our ADRs:

```
src/
├── domain/                      ← HEXAGON CENTER (Business Logic)
│   ├── OrderAggregate.ts        ← Aggregate with @CommandHandler methods
│   ├── commands/                ← Command definitions
│   │   ├── SubmitOrderCommand.ts
│   │   └── CancelOrderCommand.ts
│   └── events/                  ← Domain events
│       ├── OrderSubmittedEvent.ts  (schemaVersion: 1)
│       └── OrderCancelledEvent.ts  (schemaVersion: 1)
│
├── infrastructure/              ← APPLICATION SERVICES
│   └── CommandBus.ts            ← Routes commands to aggregates
│
└── slices/                      ← HEXAGON OUTSIDE (Adapters)
    ├── submit-order/            ← Thin CLI adapter (< 50 lines)
    │   └── SubmitOrderCli.ts
    └── cancel-order/            ← Thin CLI adapter (< 50 lines)
        └── CancelOrderCli.ts
```

### Key Principles

**ADR-004: Command Handlers in Aggregates**
- OrderAggregate contains ALL business logic
- Command handlers validate and emit events
- Event handlers update state only

**ADR-006: Domain Organization**
- Aggregates live in `domain/` (shared across slices)
- Commands in `domain/commands/`
- Events in `domain/events/`

**ADR-007: Event Versioning**
- Events have `schemaVersion` property
- Enables schema evolution over time

**ADR-008: Vertical Slices as Adapters**
- Slices are thin (< 50 lines)
- No business logic in slices
- Dispatch commands via CommandBus

### Flow Diagram

```
┌─────────────────┐
│  SubmitOrderCli │  ← Thin adapter (slice)
│   (12 lines)    │
└────────┬────────┘
         │ creates
         ▼
┌─────────────────┐
│ SubmitOrder     │  ← Command (intent)
│   Command       │
└────────┬────────┘
         │ dispatched via
         ▼
┌─────────────────┐
│   CommandBus    │  ← Infrastructure
└────────┬────────┘
         │ routes to
         ▼
┌─────────────────┐
│ OrderAggregate  │  ← Domain logic (hexagon center)
│  @CommandHandler│
│    .submit()    │  ← Validates, emits events
└────────┬────────┘
         │ emits
         ▼
┌─────────────────┐
│ OrderSubmitted  │  ← Domain event
│     Event       │
└────────┬────────┘
         │ persisted to
         ▼
┌─────────────────┐
│   EventStore    │  ← Event log (source of truth)
└─────────────────┘
```

## 🚀 Getting Started

### Prerequisites

```bash
./dev-tools/dev init   # first time only
./dev-tools/dev start  # start event store infrastructure
```

### Installation

```bash
cd examples/002-simple-aggregate-ts
pnpm install
pnpm run build
```

### Running

```bash
# Run against gRPC event store (default)
pnpm run start

# Run with in-memory event store
pnpm run start -- --memory

# Through the ADR-024 gateway (Basic auth on every call)
EVENT_STORE_ADDR=localhost:50051 ESP_GATEWAY_PASSWORD=... pnpm run start
# Remote gateway over plaintext (no TLS yet, #301): explicit opt-in
EVENT_STORE_ADDR=es.lan:50051 ESP_GATEWAY_PASSWORD=... \
  EVENT_STORE_ALLOW_INSECURE_CREDENTIALS=1 pnpm run start
```

`ESP_GATEWAY_USER` defaults to `admin`. Credentials are refused over
plaintext to a non-loopback host unless you opt in or use `https://`.

### Validate Architecture

```bash
# Validate adherence to ADRs
pnpm run validate

# Should output:
# ✅ Domain structure valid
# ✅ 1 aggregate found (OrderAggregate)
# ✅ 2 commands found
# ✅ 2 events found
# ✅ All slices are thin adapters
```

## 📖 What This Example Demonstrates

### 1. Order Lifecycle

The example demonstrates a complete order lifecycle:

1. **Submit Order** - Create new order for a customer
2. **Cancel Order** - Cancel submitted order

### 2. Business Rules

**OrderAggregate** enforces business rules:
- ✅ Orders must be submitted before cancellation
- ✅ Only submitted orders can be cancelled
- ✅ State transitions are validated

### 3. Event Sourcing

Events are the source of truth:
- `OrderSubmittedEvent` - Order was submitted
- `OrderCancelledEvent` - Order was cancelled

State is reconstructed by replaying events.

### 4. Command Pattern

Commands express intent:
- `SubmitOrderCommand` - "I want to submit an order"
- `CancelOrderCommand` - "I want to cancel an order"

Commands contain `aggregateId` for routing.

## 🔍 Code Walkthrough

### Domain: OrderAggregate

```typescript
@Aggregate("Order")
export class OrderAggregate extends AggregateRoot {
  private status: OrderStatus = OrderStatus.New;

  @CommandHandler("SubmitOrderCommand")
  submit(command: SubmitOrderCommand): void {
    // 1. Validate business rules
    if (this.status !== OrderStatus.New) {
      throw new Error("Cannot submit: Order not in New state");
    }
    
    // 2. Emit event (triggers event handler)
    this.apply(new OrderSubmittedEvent(
      command.orderId,
      command.customerId
    ));
  }

  @EventSourcingHandler("OrderSubmitted")
  private onSubmitted(event: OrderSubmittedEvent): void {
    // Update state only - no validation
    this.status = OrderStatus.Submitted;
  }
}
```

### Slice: SubmitOrderCli (Thin Adapter)

```typescript
export class SubmitOrderCli {
  constructor(private commandBus: CommandBus) {}

  async execute(orderId: string, customerId: string): Promise<void> {
    // Create command
    const command = new SubmitOrderCommand(orderId, orderId, customerId);
    
    // Dispatch to domain
    await this.commandBus.send(command);
    
    console.log(`✅ Order ${orderId} submitted`);
  }
}
```

**Notice:** Only 15 lines, no business logic!

### Infrastructure: CommandBus

```typescript
export class CommandBus {
  async send(command: any): Promise<void> {
    // 1. Load aggregate
    let aggregate = await repository.load(command.aggregateId);
    
    // 2. Dispatch command
    aggregate.handleCommand(command);
    
    // 3. Save events
    await repository.save(aggregate);
  }
}
```

## 🧪 Testing

### Manual Testing

```bash
# Run the example
pnpm run start

# Expected output:
# 🚀 002-simple-aggregate-ts (Hexagonal VSA Architecture)
# 
# 📝 Executing order lifecycle...
# 
# 1️⃣  Submit Order
# ✅ Order order-xxx submitted for customer customer-xyz
# 
# 2️⃣  Cancel Order
# ✅ Order order-xxx cancelled: customer request
# 
# ✅ Example completed successfully!
```

### Architecture Validation

```bash
pnpm run validate

# VSA CLI validates:
# ✅ Aggregates in domain/ folder
# ✅ Commands have aggregateId
# ✅ Events have schemaVersion
# ✅ Slices are < 50 lines
# ✅ No cross-slice dependencies
# ✅ Domain has no outward dependencies
```

## 📚 Related Documentation

### ADRs (Architecture Decision Records)
- [ADR-004: Command Handlers in Aggregates](../../docs/adrs/ADR-004-command-handlers-in-aggregates.md)
- [ADR-006: Domain Organization Pattern](../../docs/adrs/ADR-006-domain-organization-pattern.md)
- [ADR-007: Event Versioning and Upcasters](../../docs/adrs/ADR-007-event-versioning-upcasters.md)
- [ADR-008: Vertical Slices as Hexagonal Adapters](../../docs/adrs/ADR-008-vertical-slices-hexagonal-adapters.md)

### Other Examples
- [Example 004: CQRS Patterns](../004-cqrs-patterns-ts/) - Command/Query separation
- [Example 007: E-Commerce Complete](../007-ecommerce-complete-ts/) - Complex domain with 3 aggregates

## ❓ FAQ

**Q: Why separate commands from aggregates?**  
A: Commands are DTOs (data transfer objects) that express intent. Aggregates contain business logic. Separation enables testability and clarity.

**Q: Why use CommandBus instead of calling aggregate directly?**  
A: CommandBus handles cross-cutting concerns (transactions, logging, authorization) and provides a single entry point for commands.

**Q: Why are slices so thin?**  
A: Slices are adapters that translate external protocols (CLI, HTTP, gRPC) into domain commands. Business logic stays in the domain.

**Q: What is event versioning for?**  
A: Business requirements evolve. Event versioning allows you to change event schemas over time without breaking existing events in the store.

**Q: Can I add more business logic to slices?**  
A: ❌ No! Slices must remain thin (< 50 lines). ALL business logic belongs in the aggregate. This is enforced by `vsa validate`.

## 🔧 Troubleshooting

**Build errors?**
```bash
pnpm install
pnpm run clean
pnpm run build
```

**Event store connection failed?**
```bash
# Start dev infrastructure
./dev-tools/dev start

# Or use in-memory mode
pnpm run start -- --memory
```

**Want to verify structure?**
```bash
pnpm run validate
```

## 📄 License

MIT

---

**Next Steps:**
1. ✅ Run this example
2. ✅ Explore [Example 004: CQRS Patterns](../004-cqrs-patterns-ts/)
3. ✅ Read the ADRs to understand the architecture
4. ✅ Try modifying the example (add a new command!)
