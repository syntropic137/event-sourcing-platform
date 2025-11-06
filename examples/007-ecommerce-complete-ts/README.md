# 007-ecommerce-complete-ts — Complete E-commerce Platform

**✅ HEXAGONAL EVENT-SOURCED VSA ARCHITECTURE**

A comprehensive e-commerce platform example showcasing three fully-featured aggregates in a hexagonal event-sourced architecture, demonstrating:
- **3 Aggregates**: Product, Order, Customer
- **11 Commands** organized in `domain/commands/`
- **11 Events** with `@Event` decorators in `domain/events/`
- **CommandBus** in infrastructure layer
- **Complete order fulfillment workflow** from customer registration to shipping

## Architecture Overview

This example follows the **Hexagonal Event-Sourced VSA** pattern (ADRs 004-010):

```
src/
├── domain/                           # 🔵 CORE (Hexagon Center)
│   ├── ProductAggregate.ts           # Catalog management
│   ├── OrderAggregate.ts             # Order lifecycle (state machine)
│   ├── CustomerAggregate.ts          # Customer profile
│   ├── commands/                     # 11 commands
│   │   ├── CreateProductCommand.ts
│   │   ├── UpdateProductPriceCommand.ts
│   │   ├── AddStockCommand.ts
│   │   ├── RemoveStockCommand.ts
│   │   ├── CreateOrderCommand.ts
│   │   ├── AddOrderItemCommand.ts
│   │   ├── ConfirmOrderCommand.ts
│   │   ├── ShipOrderCommand.ts
│   │   ├── CancelOrderCommand.ts
│   │   ├── RegisterCustomerCommand.ts
│   │   └── UpdateCustomerAddressCommand.ts
│   └── events/                       # 11 events with @Event("...", "v1")
│       ├── ProductCreatedEvent.ts
│       ├── ProductPriceUpdatedEvent.ts
│       ├── StockAddedEvent.ts
│       ├── StockRemovedEvent.ts
│       ├── OrderCreatedEvent.ts
│       ├── OrderItemAddedEvent.ts
│       ├── OrderConfirmedEvent.ts
│       ├── OrderShippedEvent.ts
│       ├── OrderCancelledEvent.ts
│       ├── CustomerRegisteredEvent.ts
│       └── CustomerAddressUpdatedEvent.ts
├── infrastructure/                   # 🟢 APPLICATION SERVICES
│   └── CommandBus.ts                 # Routes commands to aggregates
└── main.ts                           # Wiring & demo

vsa.yaml                              # Architecture validation config
```

## Key Features

### 🏗️ Three Complete Aggregates

#### 1. **ProductAggregate** - Product Catalog Management
- `@CommandHandler` `createProduct()` - Create new products with validation
- `@CommandHandler` `updatePrice()` - Update product pricing
- `@CommandHandler` `addStock()` - Add inventory
- `@CommandHandler` `removeStock()` - Remove inventory (with validation)

#### 2. **OrderAggregate** - Order Lifecycle Management
- `@CommandHandler` `createOrder()` - Create draft order
- `@CommandHandler` `addItem()` - Add items to order (DRAFT only)
- `@CommandHandler` `confirmOrder()` - Confirm and calculate total
- `@CommandHandler` `shipOrder()` - Ship with tracking number
- `@CommandHandler` `cancelOrder()` - Cancel order (if not shipped)
- **State Machine**: DRAFT → CONFIRMED → SHIPPED (or CANCELLED)

#### 3. **CustomerAggregate** - Customer Management
- `@CommandHandler` `registerCustomer()` - Register new customer with email validation
- `@CommandHandler` `updateAddress()` - Update shipping address

### 🎯 Architectural Compliance

- ✅ **ADR-004**: Commands as classes with `aggregateId`, handlers in aggregates
- ✅ **ADR-006**: Domain organized in `domain/` folder with clear structure
- ✅ **ADR-007**: All events use `@Event("EventType", "v1")` decorator
- ✅ **ADR-008**: Clear separation between domain and infrastructure
- ✅ **ADR-010**: Decorator patterns for framework integration
- ✅ **Hexagonal Architecture**: Domain isolated from infrastructure
- ✅ **Business Validation**: In command handlers (price >= 0, stock validation)
- ✅ **State-Only Updates**: In event sourcing handlers
- ✅ **Complete Workflow**: Customer → Product → Order → Shipping

## Run the Example

```bash
# Memory mode (fast, no dependencies)
pnpm --filter ./examples/007-ecommerce-complete-ts run dev -- --memory

# OR with npm:
cd examples/007-ecommerce-complete-ts
npm run dev -- --memory

# gRPC mode (requires event store)
./dev-tools/dev start
pnpm --filter ./examples/007-ecommerce-complete-ts run dev
```

## Example Output

```
🛒 E-commerce Platform - Complete Example
==========================================
✅ HEXAGONAL EVENT-SOURCED VSA ARCHITECTURE

👤 DEMO: Customer Registration
===============================
✓ Customer registered: customer-001
  Email: john.doe@example.com
  Name: John Doe

📦 DEMO: Product Management
============================
✓ Product created: product-001
  Name: Wireless Mouse
  Price: $29.99
  Stock: 100 units
✓ Stock added: +50 units (now 150 units)

📋 DEMO: Order Lifecycle
=========================
✓ Order created: order-001
✓ Item added: 2x Wireless Mouse @ $29.99
✓ Order confirmed (Status: CONFIRMED)
  Total: $59.98
✓ Stock removed: -2 units (now 148 units)
✓ Order shipped (Status: SHIPPED)

🎉 Complete E-commerce Flow Demonstrated!

📊 Architecture Summary:
  ✓ 3 Aggregates: Product, Order, Customer
  ✓ 11 Commands in domain/commands/
  ✓ 11 Events with @Event decorators in domain/events/
  ✓ CommandBus in infrastructure/
  ✓ Full hexagonal architecture with VSA
  ✓ All events versioned (v1)
  ✓ Complete order fulfillment workflow

✅ ARCHITECTURE COMPLIANCE VERIFIED
```

## Validate Architecture

```bash
# Validate with VSA CLI
npm run validate

# Expected output:
# ✅ Domain structure valid
# ✅ 3 aggregates found (ProductAggregate, OrderAggregate, CustomerAggregate)
# ✅ 11 commands found
# ✅ 11 events found with @Event decorators
# ✅ All events have version "v1"
# ✅ CommandBus present in infrastructure/
# ✅ Domain has no outward dependencies
```

## Code Example: OrderAggregate

```typescript
@Aggregate("Order")
export class OrderAggregate extends AggregateRoot<OrderEvent> {
  private customerId = "";
  private items: OrderItem[] = [];
  private status = OrderStatus.DRAFT;
  private totalAmount = 0;

  // Command Handler - validates and applies events
  @CommandHandler("ConfirmOrderCommand")
  confirmOrder(command: ConfirmOrderCommand): void {
    // 1. Validation
    if (this.id === null) throw new Error("Order does not exist");
    if (this.status !== OrderStatus.DRAFT) 
      throw new Error("Order is not in DRAFT status");
    if (this.items.length === 0) 
      throw new Error("Cannot confirm empty order");
    
    // 2. Calculate total
    const total = this.items.reduce(
      (sum, item) => sum + item.quantity * item.pricePerUnit,
      0
    );
    
    // 3. Apply event
    this.apply(new OrderConfirmedEvent(total));
  }

  // Event Sourcing Handler - updates state only
  @EventSourcingHandler("OrderConfirmed")
  private onOrderConfirmed(event: OrderConfirmedEvent): void {
    this.status = OrderStatus.CONFIRMED;
    this.totalAmount = event.totalAmount;
  }
}
```

## What This Example Demonstrates

### ✅ Hexagonal Architecture
- **Domain Layer**: Pure business logic in aggregates (no dependencies)
- **Infrastructure Layer**: CommandBus routes commands to aggregates
- **Adapters**: Main entry point (could be REST API, CLI, gRPC)

### ✅ Event Sourcing
- All state changes captured as events
- Events use `@Event` decorator with versioning
- Event sourcing handlers rebuild aggregate state

### ✅ Domain-Driven Design
- 3 aggregates as consistency boundaries
- Commands express intent
- Events capture facts
- Business validation in aggregates

### ✅ Order Fulfillment Workflow
1. **Customer Registration** → CustomerAggregate
2. **Product Creation** → ProductAggregate
3. **Stock Management** → ProductAggregate
4. **Order Creation** → OrderAggregate
5. **Add Items** → OrderAggregate
6. **Confirm Order** → OrderAggregate
7. **Remove Stock** → ProductAggregate (for order)
8. **Ship Order** → OrderAggregate

## Learn More

- **ADR-004**: [Command Handlers in Aggregates](../../docs/adrs/ADR-004-command-handlers-in-aggregates.md)
- **ADR-006**: [Domain Organization Pattern](../../docs/adrs/ADR-006-domain-organization-pattern.md)
- **ADR-007**: [Event Versioning and Upcasters](../../docs/adrs/ADR-007-event-versioning-upcasters.md)
- **ADR-010**: [Decorator Patterns for Framework Integration](../../docs/adrs/ADR-010-decorator-patterns-framework.md)
- **Hexagonal Architecture**: [ADR-005](../../docs/adrs/ADR-005-hexagonal-architecture-event-sourcing.md)
