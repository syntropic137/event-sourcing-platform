---
sidebar_position: 1
---

# Vertical Slices

Learn how Vertical Slice Architecture organizes code by business features instead of technical layers.

## What is a Vertical Slice?

A **vertical slice** is a complete feature implementation that cuts through all architectural layers. Instead of organizing code by technical concerns (controllers, services, repositories), VSA organizes by business capabilities (place-order, cancel-order, add-product).

### Traditional Layered Architecture

```
src/
├── controllers/          ← All controllers together
│   ├── OrderController.ts
│   ├── ProductController.ts
│   └── UserController.ts
├── services/             ← All services together
│   ├── OrderService.ts
│   ├── ProductService.ts
│   └── UserService.ts
└── repositories/         ← All repositories together
    ├── OrderRepository.ts
    ├── ProductRepository.ts
    └── UserRepository.ts
```

**Problems:**
- 🚫 Changes ripple across directories
- 🚫 Hard to understand a complete feature
- 🚫 Difficult to work in parallel
- 🚫 Tight coupling between layers
- 🚫 Harder to test in isolation

### Vertical Slice Architecture

```
src/contexts/orders/
├── place-order/          ← Complete feature!
│   ├── PlaceOrderCommand.ts
│   ├── OrderPlacedEvent.ts
│   ├── PlaceOrderHandler.ts
│   ├── OrderAggregate.ts
│   └── PlaceOrder.test.ts
├── cancel-order/         ← Another complete feature!
│   ├── CancelOrderCommand.ts
│   ├── OrderCancelledEvent.ts
│   ├── CancelOrderHandler.ts
│   └── CancelOrder.test.ts
└── _shared/              ← Only what's truly shared
    └── OrderAggregate.ts
```

**Benefits:**
- ✅ Everything for one feature in one place
- ✅ Easy to understand and modify
- ✅ Teams can work in parallel
- ✅ Loose coupling between features
- ✅ Easy to test in isolation

## Anatomy of a Vertical Slice

Each slice contains everything needed for one operation:

### 1. Command

Represents **intent** - what we want to do.

```typescript title="PlaceOrderCommand.ts"
export class PlaceOrderCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly customerId: string,
    public readonly items: Array<{
      productId: string;
      quantity: number;
      price: number;
    }>
  ) {}
}
```

**Characteristics:**
- Present tense action (`PlaceOrder`, not `OrderPlaced`)
- Class (not interface) with `aggregateId` property
- Contains all necessary input data
- No business logic
- Immutable via `readonly` properties

### 2. Event

Represents **fact** - what happened.

```typescript title="OrderPlacedEvent.ts"
export interface OrderPlacedEvent {
  orderId: string;
  customerId: string;
  items: Array<{
    productId: string;
    quantity: number;
    price: number;
  }>;
  totalAmount: number;
  placedAt: Date;
}
```

**Characteristics:**
- Past tense (`OrderPlaced`, not `PlaceOrder`)
- Captures state changes
- Immutable (events are facts)
- May include computed values

### 3. Aggregate with Command Handlers

Aggregates handle commands and maintain state through events.

```typescript title="OrderAggregate.ts"
import { Aggregate, AggregateRoot, CommandHandler, EventSourcingHandler } from '@syntropic137/event-sourcing-typescript';

@Aggregate('Order')
export class OrderAggregate extends AggregateRoot<OrderEvent> {
  private status: OrderStatus = OrderStatus.Draft;
  private items: OrderItem[] = [];

  // COMMAND HANDLER - Validates business rules and emits events
  @CommandHandler('PlaceOrderCommand')
  placeOrder(command: PlaceOrderCommand): void {
    // 1. Validate business rules
    if (this.status !== OrderStatus.Draft) {
      throw new Error('Order already placed');
    }
    if (command.items.length === 0) {
      throw new Error('Order must have items');
    }
    if (this.id !== null) {
      throw new Error('Order already initialized');
    }

    // 2. Initialize aggregate
    this.initialize(command.aggregateId);

    // 3. Apply event
    this.apply(new OrderPlacedEvent(command.aggregateId, command.items));
  }

  // EVENT SOURCING HANDLER - Updates state only (NO business logic)
  @EventSourcingHandler('OrderPlaced')
  private onOrderPlaced(event: OrderPlacedEvent): void {
    this.status = OrderStatus.Placed;
    this.items = event.items;
  }

  getAggregateType(): string {
    return 'Order';
  }
}
```

**Command Handler Responsibilities:**
- Validate business rules using current state
- Initialize aggregate (for creation commands)
- Apply events (which trigger event handlers)
- NO direct state modification

**Event Sourcing Handler Responsibilities:**
- Update internal state only
- NO validation or business logic
- Must be idempotent
- Used for both new events and rehydration

### 4. Tests

Verify the feature works correctly.

```typescript title="PlaceOrder.test.ts"
describe('PlaceOrder', () => {
  it('should place an order successfully', async () => {
    // Arrange
    const handler = new PlaceOrderHandler(
      new InMemoryEventStore()
    );
    const command = createValidCommand();

    // Act
    await handler.handle(command);

    // Assert
    const events = await eventStore.getEvents(command.orderId);
    expect(events[0].type).toBe('OrderPlaced');
  });
});
```

**Test Types:**
- Unit tests (fast, isolated)
- Integration tests (with dependencies)
- E2E tests (full workflow)

## Benefits of Vertical Slices

### 1. High Cohesion

Related code stays together:

```
place-order/
├── PlaceOrderCommand.ts      ← Related
├── OrderPlacedEvent.ts        ← to
├── PlaceOrderHandler.ts       ← the
├── OrderAggregate.ts          ← same
└── PlaceOrder.test.ts         ← feature
```

**Impact:** Changes stay localized, easier to understand and modify.

### 2. Low Coupling

Features don't depend on each other:

```typescript
// ❌ Tight coupling (layered)
class OrderService {
  constructor(
    private userService: UserService,  // Depends on other services
    private productService: ProductService,
    private paymentService: PaymentService
  ) {}
}

// ✅ Loose coupling (vertical slice)
class PlaceOrderHandler {
  constructor(
    private eventStore: EventStore,  // Depends on infrastructure only
  ) {}
}
```

**Impact:** Changes to one feature don't break others.

### 3. Parallel Development

Teams can work independently:

```
Team A:        Team B:              Team C:
place-order/   cancel-order/        update-shipping/
├── ...        ├── ...              ├── ...
└── ...        └── ...              └── ...
```

**Impact:** No merge conflicts, faster development.

### 4. Easy Testing

Test one feature in isolation:

```typescript
// Each slice has its own tests
describe('PlaceOrder', () => {
  // Tests only PlaceOrder feature
});

describe('CancelOrder', () => {
  // Tests only CancelOrder feature
});
```

**Impact:** Faster tests, easier debugging.

### 5. Clear Ownership

One team owns complete features:

```
orders-team owns:
  - place-order/
  - cancel-order/
  - update-order/

shipping-team owns:
  - schedule-shipment/
  - track-package/
```

**Impact:** Clear responsibilities, better accountability.

## When to Use Vertical Slices

### ✅ Good Fits

- **CQRS applications** - Commands map naturally to slices
- **Event-sourced systems** - Events are part of slice
- **Microservices** - Each slice can become a service
- **Feature-driven development** - Aligns with user stories
- **Team autonomy** - Teams own complete features

### ⚠️ Consider Carefully

- **CRUD-only apps** - May be overkill for simple CRUD
- **Small applications** - Overhead might not be worth it
- **Shared data models** - Requires duplication or shared code
- **Legacy migrations** - Gradual adoption needed

## Common Patterns

### Pattern 1: Thin Slices

Keep slices focused on one operation:

```
✅ Good: Focused slices
place-order/
cancel-order/
update-order-status/

❌ Bad: Fat slices
order-management/  ← Too broad
  - Place
  - Cancel
  - Update
  - Query
```

### Pattern 2: Shared Code

Keep sharing minimal:

```
contexts/orders/
├── place-order/
├── cancel-order/
└── _shared/              ← Only truly shared code
    ├── OrderAggregate.ts ← Used by multiple slices
    └── OrderValidator.ts ← Shared validation
```

### Pattern 3: Query Slices

Separate command and query slices:

```
contexts/orders/
├── place-order/          ← Command (write)
├── cancel-order/         ← Command (write)
└── _queries/             ← Queries (read)
    ├── get-order/
    └── list-orders/
```

## Comparison with Other Architectures

| Aspect | Layered | Vertical Slice |
|--------|---------|----------------|
| **Organization** | By technical layer | By business feature |
| **Change Impact** | Ripples across layers | Localized to slice |
| **Team Work** | Sequential (waiting) | Parallel (independent) |
| **Coupling** | High (layers depend on each other) | Low (slices independent) |
| **Testing** | Complex (many dependencies) | Simple (isolated) |
| **Onboarding** | Need to understand all layers | Understand one slice at a time |

## Pitfalls to Avoid

### 1. Shared State

```typescript
// ❌ Avoid: Shared mutable state
export const orderCache = new Map();

// ✅ Better: Each slice manages own state
class PlaceOrderHandler {
  private cache = new Map();
}
```

### 2. Cross-Slice Dependencies

```typescript
// ❌ Avoid: Direct slice dependencies
import { CancelOrderHandler } from '../cancel-order/CancelOrderHandler';

// ✅ Better: Use events or shared abstractions
await this.eventBus.publish(new OrderCancellationRequested());
```

### 3. Duplication Fear

```typescript
// ⚠️ Don't fear some duplication
// Each slice can have its own validation

// It's OK if place-order/ and cancel-order/ 
// both validate order IDs differently
```

**Principle:** Prefer duplication over wrong abstraction.

## Migration Strategy

### From Layered to Vertical Slices

1. **Start with one feature**
   ```
   Create new slice: place-order/
   Keep existing layers temporarily
   ```

2. **Move code incrementally**
   ```
   Move controller logic → Handler
   Move service logic → Handler/Aggregate
   Move repository logic → Event Store
   ```

3. **Add tests**
   ```
   Write slice tests
   Keep integration tests
   ```

4. **Repeat for other features**
   ```
   One feature at a time
   Validate each migration
   ```

## Real-World Examples

### E-Commerce Order Management

```
contexts/orders/
├── place-order/
│   ├── PlaceOrderCommand.ts
│   ├── OrderPlacedEvent.ts
│   ├── PlaceOrderHandler.ts
│   └── PlaceOrder.test.ts
├── cancel-order/
│   ├── CancelOrderCommand.ts
│   ├── OrderCancelledEvent.ts
│   ├── CancelOrderHandler.ts
│   └── CancelOrder.test.ts
└── update-shipping-address/
    ├── UpdateShippingAddressCommand.ts
    ├── ShippingAddressUpdatedEvent.ts
    ├── UpdateShippingAddressHandler.ts
    └── UpdateShippingAddress.test.ts
```

### Library Management

```
contexts/catalog/
├── add-book/
├── remove-book/
└── update-book-details/

contexts/lending/
├── borrow-book/
├── return-book/
└── mark-overdue/
```

## Next Steps

- **[Bounded Contexts](./bounded-contexts)** - Organize slices into contexts
- **[Integration Events](./integration-events)** - Communication between contexts
- **[Convention Over Configuration](./convention-over-configuration)** - Standard patterns
- **[Your First Feature](../getting-started/your-first-feature)** - Build a slice

## Resources

- [Vertical Slice Architecture - Jimmy Bogard](https://www.jimmybogard.com/vertical-slice-architecture/)
- [Feature Folders - Oskar Dudycz](https://event-driven.io/en/how_to_slice_the_codebase_effectively/)
- [CQRS + Vertical Slices](https://www.youtube.com/watch?v=SUiWfhAhgQw)

---

**Ready to build?** Start with [Your First Feature](../getting-started/your-first-feature) to create your first vertical slice.

