---
sidebar_position: 2
---

# Quick Start

Build your first VSA project in 10 minutes. This guide walks you through creating a simple order management system using Vertical Slice Architecture.

## What You'll Build

A simple order management system with:
- Place order feature (command → event → handler)
- Event sourcing integration
- Comprehensive tests
- Proper VSA structure

## Step 1: Create a New Project

```bash
# Create and enter project directory
mkdir order-system
cd order-system

# Initialize VSA with TypeScript
vsa init --language typescript

# Install dependencies
npm install
```

This creates:

```
order-system/
├── vsa.yaml           # VSA configuration
├── package.json
└── src/
    └── contexts/      # Root for vertical slices
```

## Step 2: Configure Your Project

Edit `vsa.yaml`:

```yaml
version: 1
language: typescript
root: src/contexts

# Define your first bounded context
bounded_contexts:
  - name: orders
    description: Order management and processing
    publishes: []
    subscribes: []

# Validation rules
validation:
  require_tests: true
  require_handler: true
  require_aggregate: false
```

## Step 3: Generate Your First Feature

```bash
# Generate the place-order feature
vsa generate orders place-order
```

This creates a complete vertical slice:

```
src/contexts/orders/place-order/
├── PlaceOrderCommand.ts     # What we want to do
├── OrderPlacedEvent.ts      # What happened
├── PlaceOrderHandler.ts     # Business logic
└── PlaceOrder.test.ts       # Tests
```

## Step 4: Implement the Command

Edit `src/contexts/orders/place-order/PlaceOrderCommand.ts`:

```typescript
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

:::tip Why Commands are Classes
Commands must be classes (not interfaces) to work with `@CommandHandler`:
- The `aggregateId` property identifies which aggregate instance to load
- Classes enable proper metadata reflection via decorators
- TypeScript compiler can validate at build time
:::

## Step 5: Define the Event

Edit `src/contexts/orders/place-order/OrderPlacedEvent.ts`:

```typescript
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

## Step 6: Implement Business Logic

First, install the event-sourcing platform SDK:

```bash
npm install @event-sourcing-platform/typescript
```

Create the aggregate `src/contexts/orders/place-order/OrderAggregate.ts`:

```typescript
import { 
  AggregateRoot, 
  Aggregate,
  CommandHandler,
  EventSourcingHandler,
  BaseDomainEvent 
} from '@event-sourcing-platform/typescript';
import { PlaceOrderCommand } from './PlaceOrderCommand';
import { OrderPlacedEvent } from './OrderPlacedEvent';

// Define event as class
class OrderPlaced extends BaseDomainEvent {
  readonly eventType = 'OrderPlaced' as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public orderId: string,
    public customerId: string,
    public items: Array<{ productId: string; quantity: number; price: number }>,
    public totalAmount: number
  ) {
    super();
  }
}

@Aggregate('Order')
export class OrderAggregate extends AggregateRoot<OrderPlaced> {
  private customerId: string | null = null;
  private items: Array<{ productId: string; quantity: number; price: number }> = [];
  private totalAmount: number = 0;

  // COMMAND HANDLER - Validates business rules and emits events
  @CommandHandler('PlaceOrderCommand')
  placeOrder(command: PlaceOrderCommand): void {
    // 1. Validate business rules
    if (!command.items || command.items.length === 0) {
      throw new Error('Order must contain at least one item');
    }
    if (!command.customerId) {
      throw new Error('Customer ID is required');
    }
    if (this.id !== null) {
      throw new Error('Order already placed');
    }

    // 2. Initialize aggregate (required before raising events)
    this.initialize(command.aggregateId);

    // 3. Calculate total
    const totalAmount = command.items.reduce(
      (sum, item) => sum + item.price * item.quantity,
      0
    );

    // 4. Apply event
    this.apply(new OrderPlaced(
      command.aggregateId,
      command.customerId,
      command.items,
      totalAmount
    ));
  }

  // EVENT SOURCING HANDLER - Updates state only (NO validation)
  @EventSourcingHandler('OrderPlaced')
  private onOrderPlaced(event: OrderPlaced): void {
    this.customerId = event.customerId;
    this.items = event.items;
    this.totalAmount = event.totalAmount;
  }

  // Getters
  getCustomerId(): string | null { return this.customerId; }
  getItems() { return this.items; }
  getTotalAmount(): number { return this.totalAmount; }

  getAggregateType(): string {
    return 'Order';
  }
}
```

:::tip The @CommandHandler Pattern

Commands are handled directly on the aggregate using `@CommandHandler`:

- **Command Handler**: Validates rules, initializes aggregate, emits events
- **Event Sourcing Handler**: Updates state only (no validation)
- **Separation of Concerns**: Business logic vs state updates are clearly separated

This is the industry-standard pattern from "Understanding Event Sourcing" and frameworks like Axon.

:::

:::info Why This Pattern?

1. **Encapsulation** - Business rules stay with aggregate state
2. **Testability** - Test aggregates in isolation, no infrastructure needed
3. **Event Sourcing** - Full audit trail of all changes
4. **CQRS** - Separate write model (aggregates) from read models
5. **Less Code** - No separate handler classes needed

:::


## Step 7: Write Tests

Edit `src/contexts/orders/place-order/PlaceOrder.test.ts`:

```typescript
import { OrderAggregate } from './OrderAggregate';
import { PlaceOrderCommand } from './PlaceOrderCommand';

describe('PlaceOrder', () => {
  it('should place an order successfully', () => {
    // Arrange
    const aggregate = new OrderAggregate();
    const command = new PlaceOrderCommand(
      'order-123',
      'customer-456',
      [
        { productId: 'product-1', quantity: 2, price: 10.0 },
        { productId: 'product-2', quantity: 1, price: 15.0 },
      ]
    );

    // Act
    (aggregate as any).handleCommand(command);

    // Assert - Check uncommitted events
    const events = aggregate.getUncommittedEvents();
    expect(events).toHaveLength(1);
    
    const event = events[0];
    expect(event.eventType).toBe('OrderPlaced');
    expect(event.customerId).toBe('customer-456');
    expect(event.items).toHaveLength(2);
    expect(event.totalAmount).toBe(35.0); // (2 * 10) + (1 * 15)
  });

  it('should reject orders with no items', () => {
    // Arrange
    const aggregate = new OrderAggregate();
    const command = new PlaceOrderCommand('order-123', 'customer-456', []);

    // Act & Assert
    expect(() => (aggregate as any).handleCommand(command)).toThrow(
      'Order must contain at least one item'
    );
  });

  it('should reject orders without customer', () => {
    // Arrange
    const aggregate = new OrderAggregate();
    const command = new PlaceOrderCommand('order-123', '', [
      { productId: 'product-1', quantity: 1, price: 10.0 }
    ]);

    // Act & Assert
    expect(() => (aggregate as any).handleCommand(command)).toThrow(
      'Customer ID is required'
    );
  });

  it('should reject placing same order twice', () => {
    // Arrange
    const aggregate = new OrderAggregate();
    const command = new PlaceOrderCommand('order-123', 'customer-456', [
      { productId: 'product-1', quantity: 1, price: 10.0 }
    ]);

    // Act - Place order first time (should succeed)
    (aggregate as any).handleCommand(command);

    // Assert - Try to place again (should fail)
    expect(() => (aggregate as any).handleCommand(command)).toThrow(
      'Order already placed'
    );
  });
});
```

:::tip Testing Aggregates

Notice how the tests:

1. **Test aggregates in isolation** - No infrastructure needed
2. **Verify events are emitted** - Check uncommitted events
3. **Enforce business rules** - Validation works correctly
4. **Are fast** - No database or event store required

This pattern enables true Test-Driven Development (TDD).

:::

## Step 8: Validate Your Structure

```bash
# Run VSA validation
vsa validate
```

Expected output:

```
✅ Validating: src/contexts

📦 Context: orders
  ✅ place-order
     ├─ PlaceOrderCommand.ts
     ├─ OrderPlacedEvent.ts
     ├─ PlaceOrderHandler.ts
     └─ PlaceOrder.test.ts

✅ Summary: 1 feature validated, 0 errors
```

## Step 9: Run Tests

```bash
# Install test dependencies
npm install --save-dev jest @types/jest ts-jest

# Configure Jest (if needed)
npx ts-jest config:init

# Run tests
npm test
```

Expected output:

```
 PASS  src/contexts/orders/place-order/PlaceOrder.test.ts
  PlaceOrder
    ✓ should place an order successfully (5 ms)
    ✓ should reject orders with no items (2 ms)
    ✓ should reject orders without customer (1 ms)

Test Suites: 1 passed, 1 total
Tests:       3 passed, 3 total
```

## Step 10: Enable Watch Mode (Optional)

For real-time validation while developing:

```bash
# Terminal 1: Run tests in watch mode
npm test -- --watch

# Terminal 2: Run VSA validation in watch mode
vsa validate --watch
```

Now any file changes will trigger automatic validation and tests!

## Project Structure Review

Your project now looks like this:

```
order-system/
├── vsa.yaml
├── package.json
├── jest.config.js
└── src/
    └── contexts/
        └── orders/
            └── place-order/
                ├── PlaceOrderCommand.ts
                ├── OrderPlacedEvent.ts
                ├── PlaceOrderHandler.ts
                └── PlaceOrder.test.ts
```

## Understanding the Vertical Slice

Each feature is a **complete vertical slice**:

1. **Command** (`PlaceOrderCommand.ts`) - Defines what we want to do
2. **Event** (`OrderPlacedEvent.ts`) - Defines what happened
3. **Handler** (`PlaceOrderHandler.ts`) - Contains business logic
4. **Test** (`PlaceOrder.test.ts`) - Verifies behavior

This is different from traditional layered architecture where you'd have:
- Commands in one directory
- Events in another
- Handlers in a third
- Tests scattered everywhere

With VSA, **everything for one feature is in one place**!

## Key Benefits You Just Experienced

✅ **Fast Scaffolding** - `vsa generate` created all files instantly

✅ **Validated Structure** - `vsa validate` ensures consistency

✅ **Self-Contained** - All feature code in one directory

✅ **Easy to Test** - Tests live with the code they test

✅ **Clear Intent** - File names tell you exactly what they do

## Next Steps

### Add More Features

```bash
# Generate another feature
vsa generate orders cancel-order

# Validate
vsa validate
```

### Add Event Sourcing

Integrate with the event store:

```bash
npm install @event-sourcing-platform/sdk-ts
```

See [Event Sourcing Integration](../advanced/event-sourcing-integration) for details.

### Add More Contexts

Edit `vsa.yaml` to add bounded contexts:

```yaml
bounded_contexts:
  - name: orders
    description: Order management
  - name: shipping
    description: Shipment processing
  - name: inventory
    description: Stock management
```

### Explore Examples

Check out complete working examples in the `vsa/examples/` directory:

- **Todo List** (`01-todo-list-ts`) - Simple single-context app
- **Library Management** (`02-library-management-ts`) - Multi-context with integration events

See the repository for complete source code and additional examples.

## Common Next Actions

### List All Features

```bash
vsa list
```

### Generate Manifest

```bash
vsa manifest --output ARCHITECTURE.md
```

### Watch Mode During Development

```bash
# Keep this running while coding
vsa validate --watch
```

## What You Learned

In this quick start, you:

1. ✅ Created a VSA project
2. ✅ Configured bounded contexts
3. ✅ Generated a vertical slice
4. ✅ Implemented business logic
5. ✅ Wrote comprehensive tests
6. ✅ Validated architecture
7. ✅ Understood VSA benefits

## Troubleshooting

### Validation Fails

```bash
# Get detailed output
vsa validate --verbose

# Check for naming conventions
# Commands: *Command.ts
# Events: *Event.ts
# Handlers: *Handler.ts
# Tests: *.test.ts
```

### Tests Fail

```bash
# Ensure Jest is configured
npx ts-jest config:init

# Check package.json has test script
npm run test
```

### Import Errors

Use relative imports within the same slice:

```typescript
import { PlaceOrderCommand } from './PlaceOrderCommand';
import { OrderPlacedEvent } from './OrderPlacedEvent';
```

## Resources

- **[Your First Feature](./your-first-feature)** - Deeper dive into feature development
- **[CLI Commands](../cli/commands)** - All available commands
- **[Core Concepts](../concepts/vertical-slices)** - Understanding VSA principles

---

**Congratulations!** 🎉 You've built your first VSA feature. Continue to [Your First Feature](./your-first-feature) for a deeper understanding.

