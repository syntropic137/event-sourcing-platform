---
sidebar_position: 3
---

# Your First Feature

Learn how to build a complete vertical slice with event sourcing, aggregates, and comprehensive testing.

## Overview

In this guide, we'll build a **Product Catalog** feature that:
- Adds products to a catalog
- Uses event sourcing for persistence
- Includes aggregate logic
- Has unit and integration tests
- Follows VSA conventions

## Prerequisites

- VSA CLI installed
- Basic understanding of event sourcing
- Node.js project set up

## Step 1: Set Up the Project

```bash
mkdir product-catalog
cd product-catalog
vsa init --language typescript

# Install event store SDK
npm install @event-sourcing-platform/sdk-ts
npm install --save-dev jest @types/jest ts-jest
```

Configure `vsa.yaml`:

```yaml
version: 1
language: typescript
root: src/contexts

framework:
  name: event-sourcing-platform
  aggregate_class: AggregateRoot
  aggregate_import: "@event-sourcing-platform/typescript"

bounded_contexts:
  - name: catalog
    description: Product catalog management

validation:
  require_tests: true
  require_handler: true
  require_aggregate: true  # We want aggregates for this example
```

## Step 2: Generate the Feature

```bash
vsa generate catalog add-product --with-aggregate
```

This creates:

```
src/contexts/catalog/add-product/
├── AddProductCommand.ts
├── ProductAddedEvent.ts
├── AddProductHandler.ts
├── ProductAggregate.ts
└── AddProduct.test.ts
```

## Step 3: Define the Command

Commands represent **intent** - what we want to do.

```typescript title="src/contexts/catalog/add-product/AddProductCommand.ts"
export interface AddProductCommand {
  productId: string;
  name: string;
  description: string;
  price: number;
  category: string;
  stock: number;
}
```

**Design tips:**
- Use clear, business-focused names
- Include all necessary data
- Keep it simple - just the facts

## Step 4: Define the Event

Events represent **facts** - what actually happened.

```typescript title="src/contexts/catalog/add-product/ProductAddedEvent.ts"
export interface ProductAddedEvent {
  productId: string;
  name: string;
  description: string;
  price: number;
  category: string;
  initialStock: number;
  addedAt: Date;
  addedBy?: string;
}
```

**Event design tips:**
- Past tense (ProductAdded, not AddProduct)
- Include timestamp
- Capture all relevant state changes
- Events are immutable - never modify them

## Step 5: Create the Aggregate

Aggregates enforce business rules and maintain consistency.

```typescript title="src/contexts/catalog/add-product/ProductAggregate.ts"
import { ProductAddedEvent } from './ProductAddedEvent';

export class ProductAggregate {
  private productId: string = '';
  private name: string = '';
  private price: number = 0;
  private stock: number = 0;
  private isActive: boolean = false;

  // Apply the event to update aggregate state
  applyProductAdded(event: ProductAddedEvent): void {
    this.productId = event.productId;
    this.name = event.name;
    this.price = event.price;
    this.stock = event.initialStock;
    this.isActive = true;
  }

  // Business rules
  validateAddProduct(
    name: string,
    price: number,
    stock: number
  ): void {
    if (this.isActive) {
      throw new Error('Product already exists');
    }

    if (!name || name.trim().length === 0) {
      throw new Error('Product name is required');
    }

    if (price < 0) {
      throw new Error('Price cannot be negative');
    }

    if (stock < 0) {
      throw new Error('Stock cannot be negative');
    }
  }

  // Getters for read-only access
  getProductId(): string {
    return this.productId;
  }

  isProductActive(): boolean {
    return this.isActive;
  }
}
```

**Aggregate patterns:**
- Encapsulate business rules
- Apply events to update state
- Validate before creating events
- Never expose internal state for mutation

## Step 6: Implement the Handler

Handlers coordinate the flow: validate → create event → persist.

```typescript title="src/contexts/catalog/add-product/AddProductHandler.ts"
import { AddProductCommand } from './AddProductCommand';
import { ProductAddedEvent } from './ProductAddedEvent';
import { ProductAggregate } from './ProductAggregate';

interface EventStore {
  load(aggregateId: string): Promise<any[]>;
  append(aggregateId: string, events: any[]): Promise<void>;
}

export class AddProductHandler {
  constructor(private eventStore: EventStore) {}

  async handle(command: AddProductCommand): Promise<void> {
    // 1. Load aggregate from event store
    const aggregate = new ProductAggregate();
    const events = await this.eventStore.load(command.productId);
    
    // Replay events to rebuild state
    for (const event of events) {
      if (event.type === 'ProductAdded') {
        aggregate.applyProductAdded(event.data);
      }
    }

    // 2. Validate business rules
    aggregate.validateAddProduct(
      command.name,
      command.price,
      command.stock
    );

    // 3. Create event
    const event: ProductAddedEvent = {
      productId: command.productId,
      name: command.name,
      description: command.description,
      price: command.price,
      category: command.category,
      initialStock: command.stock,
      addedAt: new Date(),
    };

    // 4. Apply event to aggregate (for consistency)
    aggregate.applyProductAdded(event);

    // 5. Persist event
    await this.eventStore.append(command.productId, [
      { type: 'ProductAdded', data: event },
    ]);
  }
}
```

**Handler responsibilities:**
1. Load aggregate from event store
2. Replay events to rebuild state
3. Validate command against business rules
4. Create event(s)
5. Apply event(s) to aggregate
6. Persist event(s)

## Step 7: Write Comprehensive Tests

```typescript title="src/contexts/catalog/add-product/AddProduct.test.ts"
import { AddProductHandler } from './AddProductHandler';
import { AddProductCommand } from './AddProductCommand';
import { ProductAggregate } from './ProductAggregate';

// In-memory event store for testing
class InMemoryEventStore {
  private events: Map<string, any[]> = new Map();

  async load(aggregateId: string): Promise<any[]> {
    return this.events.get(aggregateId) || [];
  }

  async append(aggregateId: string, events: any[]): Promise<void> {
    const existing = this.events.get(aggregateId) || [];
    this.events.set(aggregateId, [...existing, ...events]);
  }

  getEvents(aggregateId: string): any[] {
    return this.events.get(aggregateId) || [];
  }
}

describe('AddProduct', () => {
  let eventStore: InMemoryEventStore;
  let handler: AddProductHandler;

  beforeEach(() => {
    eventStore = new InMemoryEventStore();
    handler = new AddProductHandler(eventStore);
  });

  describe('Handler', () => {
    it('should add a product successfully', async () => {
      // Arrange
      const command: AddProductCommand = {
        productId: 'prod-123',
        name: 'Laptop',
        description: 'High-performance laptop',
        price: 999.99,
        category: 'Electronics',
        stock: 10,
      };

      // Act
      await handler.handle(command);

      // Assert
      const events = eventStore.getEvents('prod-123');
      expect(events).toHaveLength(1);
      expect(events[0].type).toBe('ProductAdded');
      expect(events[0].data.name).toBe('Laptop');
      expect(events[0].data.price).toBe(999.99);
    });

    it('should reject negative price', async () => {
      // Arrange
      const command: AddProductCommand = {
        productId: 'prod-123',
        name: 'Laptop',
        description: 'High-performance laptop',
        price: -100,
        category: 'Electronics',
        stock: 10,
      };

      // Act & Assert
      await expect(handler.handle(command)).rejects.toThrow(
        'Price cannot be negative'
      );
    });

    it('should reject empty product name', async () => {
      // Arrange
      const command: AddProductCommand = {
        productId: 'prod-123',
        name: '',
        description: 'Description',
        price: 100,
        category: 'Electronics',
        stock: 10,
      };

      // Act & Assert
      await expect(handler.handle(command)).rejects.toThrow(
        'Product name is required'
      );
    });

    it('should reject duplicate product', async () => {
      // Arrange
      const command: AddProductCommand = {
        productId: 'prod-123',
        name: 'Laptop',
        description: 'High-performance laptop',
        price: 999.99,
        category: 'Electronics',
        stock: 10,
      };

      // Act - Add product twice
      await handler.handle(command);

      // Assert
      await expect(handler.handle(command)).rejects.toThrow(
        'Product already exists'
      );
    });
  });

  describe('Aggregate', () => {
    it('should apply ProductAdded event', () => {
      // Arrange
      const aggregate = new ProductAggregate();
      const event = {
        productId: 'prod-123',
        name: 'Laptop',
        description: 'High-performance laptop',
        price: 999.99,
        category: 'Electronics',
        initialStock: 10,
        addedAt: new Date(),
      };

      // Act
      aggregate.applyProductAdded(event);

      // Assert
      expect(aggregate.getProductId()).toBe('prod-123');
      expect(aggregate.isProductActive()).toBe(true);
    });

    it('should validate business rules', () => {
      // Arrange
      const aggregate = new ProductAggregate();

      // Act & Assert
      expect(() => aggregate.validateAddProduct('', 100, 10)).toThrow(
        'Product name is required'
      );
      
      expect(() => aggregate.validateAddProduct('Laptop', -100, 10)).toThrow(
        'Price cannot be negative'
      );
      
      expect(() => aggregate.validateAddProduct('Laptop', 100, -5)).toThrow(
        'Stock cannot be negative'
      );
    });
  });
});
```

## Step 8: Validate and Test

```bash
# Validate VSA structure
vsa validate

# Run tests
npm test

# Watch mode for development
vsa validate --watch &
npm test -- --watch
```

## Step 9: Add Integration Test (Optional)

Test with a real event store:

```typescript title="src/contexts/catalog/add-product/AddProduct.integration.test.ts"
import { EventStoreClient } from '@event-sourcing-platform/sdk-ts';
import { AddProductHandler } from './AddProductHandler';

describe('AddProduct Integration', () => {
  let client: EventStoreClient;
  let handler: AddProductHandler;

  beforeAll(async () => {
    client = new EventStoreClient('localhost:50051');
    await client.connect();
  });

  afterAll(async () => {
    await client.disconnect();
  });

  beforeEach(() => {
    handler = new AddProductHandler(client);
  });

  it('should persist product to event store', async () => {
    const command = {
      productId: `prod-${Date.now()}`,
      name: 'Integration Test Product',
      description: 'Test description',
      price: 99.99,
      category: 'Test',
      stock: 5,
    };

    await handler.handle(command);

    // Verify event was persisted
    const events = await client.load(command.productId);
    expect(events).toHaveLength(1);
    expect(events[0].type).toBe('ProductAdded');
  });
});
```

## Key Concepts Reviewed

### 1. Vertical Slice

Everything for one feature in one place:
- ✅ Command (intent)
- ✅ Event (fact)
- ✅ Handler (coordination)
- ✅ Aggregate (rules)
- ✅ Tests (verification)

### 2. Event Sourcing

State is rebuilt from events:
1. Load events from store
2. Replay events through aggregate
3. Validate new command
4. Create and persist new event

### 3. Command-Query Separation

- **Commands** change state (no return value)
- **Queries** read state (no side effects)
- This example is a command

### 4. Testing Strategy

- **Unit tests** - Fast, in-memory, no dependencies
- **Integration tests** - Real event store, slower
- **Both are important!**

## Common Patterns

### Pattern 1: Validation in Aggregate

```typescript
// Good: Business rules in aggregate
aggregate.validateAddProduct(name, price, stock);

// Bad: Validation scattered in handler
if (price < 0) throw new Error('...');
```

### Pattern 2: Event Replay

```typescript
// Always replay events to rebuild state
for (const event of events) {
  aggregate.apply(event);
}
```

### Pattern 3: Idempotency

```typescript
// Check if command already executed
if (aggregate.isActive()) {
  throw new Error('Already processed');
}
```

## Next Steps

### Add More Features

```bash
# Add complementary features
vsa generate catalog update-product-price
vsa generate catalog remove-product
vsa generate catalog adjust-stock
```

### Add Queries

Create read models for querying:

```bash
mkdir src/contexts/catalog/_queries
# Implement projections and read models
```

### Add Integration Events

Publish events to other contexts:

```yaml
# vsa.yaml
bounded_contexts:
  - name: catalog
    publishes:
      - ProductAdded
      - ProductRemoved
```

### Explore Advanced Patterns

- [Bounded Contexts](../concepts/bounded-contexts) - Multiple context boundaries
- [Integration Events](../concepts/integration-events) - Cross-context communication
- Saga Orchestration - Complex workflows (advanced topic)
- Testing Strategies - Comprehensive testing (advanced topic)

## Troubleshooting

### Event Store Connection Fails

```typescript
// Add retry logic
const client = new EventStoreClient('localhost:50051', {
  maxRetries: 3,
  retryDelay: 1000,
});
```

### Aggregate State Inconsistent

```typescript
// Always apply events in order
events.sort((a, b) => a.version - b.version);
for (const event of events) {
  aggregate.apply(event);
}
```

### Tests Are Slow

```typescript
// Use in-memory store for unit tests
// Only use real store for integration tests
describe('Unit Tests', () => {
  // Fast tests with InMemoryEventStore
});

describe('Integration Tests', () => {
  // Slower tests with real EventStore
});
```

## Summary

You've learned how to:

✅ Generate a feature with aggregate
✅ Define commands and events
✅ Implement business rules in aggregates
✅ Coordinate with handlers
✅ Write comprehensive tests
✅ Use event sourcing for persistence
✅ Follow VSA conventions

**Ready for more?** Explore the complete examples in the repository:
- Todo List Example (`vsa/examples/01-todo-list-ts`)
- Library Management Example (`vsa/examples/02-library-management-ts`)

---

**Congratulations!** 🎉 You've built a production-ready vertical slice with event sourcing.

