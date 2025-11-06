---
sidebar_position: 1
---

# Event Sourcing Integration

Integrate VSA with event sourcing frameworks and event stores.

## Overview

VSA works seamlessly with event sourcing patterns. This guide shows how to integrate with the event-sourcing-platform or other event stores.

## Why Event Sourcing with VSA?

**Natural Fit:**
- Commands → Events mapping
- Aggregates enforce business rules
- Temporal queries and audit trails
- Event-driven architecture
- Testability and replay

## Architecture

```
Command → Aggregate (@CommandHandler) → Events → Event Store
                  ↓
          Business Rules + State
```

Commands are handled directly on aggregates using `@CommandHandler` decorators. No separate handler classes needed.

## Configuration

### vsa.yaml

```yaml
version: 1
language: typescript
root: src/contexts

framework:
  name: event-sourcing-platform
  aggregate_class: AggregateRoot
  aggregate_import: "@event-sourcing-platform/typescript"

bounded_contexts:
  - name: orders
    description: Order processing
```

## Implementation

### Event Store Adapter

```typescript
// src/infrastructure/EventStoreAdapter.ts
import { EventStoreClient } from '@event-sourcing-platform/sdk-ts';

export class EventStoreAdapter {
  private client: EventStoreClient;

  constructor(connectionString: string) {
    this.client = new EventStoreClient(connectionString);
  }

  async append(aggregateId: string, events: any[]): Promise<void> {
    await this.client.appendEvents(aggregateId, events);
  }

  async load(aggregateId: string): Promise<any[]> {
    return await this.client.loadEvents(aggregateId);
  }
}
```

### Command as Class

```typescript
// contexts/orders/place-order/PlaceOrderCommand.ts
export class PlaceOrderCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly customerId: string,
    public readonly items: Array<{ productId: string; quantity: number; price: number }>
  ) {}
}
```

### Aggregate with Command Handlers

```typescript
// contexts/orders/place-order/OrderAggregate.ts
import { 
  Aggregate, 
  AggregateRoot, 
  CommandHandler, 
  EventSourcingHandler,
  BaseDomainEvent 
} from '@event-sourcing-platform/typescript';

class OrderPlacedEvent extends BaseDomainEvent {
  readonly eventType = 'OrderPlaced' as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public orderId: string,
    public customerId: string,
    public items: Array<{ productId: string; quantity: number; price: number }>
  ) {
    super();
  }
}

@Aggregate('Order')
export class OrderAggregate extends AggregateRoot<OrderPlacedEvent> {
  private customerId: string | null = null;
  private items: Array<{ productId: string; quantity: number; price: number }> = [];

  // COMMAND HANDLER - Business logic and validation
  @CommandHandler('PlaceOrderCommand')
  placeOrder(command: PlaceOrderCommand): void {
    // Validate
    if (!command.items || command.items.length === 0) {
      throw new Error('Order must have items');
    }
    if (this.id !== null) {
      throw new Error('Order already placed');
    }

    // Initialize and emit event
    this.initialize(command.aggregateId);
    this.apply(new OrderPlacedEvent(
      command.aggregateId,
      command.customerId,
      command.items
    ));
  }

  // EVENT SOURCING HANDLER - State updates only
  @EventSourcingHandler('OrderPlaced')
  private onOrderPlaced(event: OrderPlacedEvent): void {
    this.customerId = event.customerId;
    this.items = event.items;
  }

  getAggregateType(): string {
    return 'Order';
  }
}
```

### Repository Pattern

```typescript
// src/infrastructure/CommandBus.ts
import { RepositoryFactory, EventStoreClient } from '@event-sourcing-platform/typescript';
import { OrderAggregate } from '../contexts/orders/place-order/OrderAggregate';
import { PlaceOrderCommand } from '../contexts/orders/place-order/PlaceOrderCommand';

export class CommandBus {
  private repository: Repository<OrderAggregate>;

  constructor(eventStoreClient: EventStoreClient) {
    const factory = new RepositoryFactory(eventStoreClient);
    this.repository = factory.createRepository(
      () => new OrderAggregate(),
      'Order'
    );
  }

  async send(command: PlaceOrderCommand): Promise<void> {
    // Load or create aggregate
    let aggregate = await this.repository.load(command.aggregateId);
    if (!aggregate) {
      aggregate = new OrderAggregate();
    }

    // Dispatch to @CommandHandler
    aggregate.handleCommand(command);

    // Save (includes optimistic concurrency check)
    await this.repository.save(aggregate);
  }
}
```

## Testing

### Unit Tests (Aggregate Testing)

Test aggregates in complete isolation - no infrastructure needed:

```typescript
import { OrderAggregate } from './OrderAggregate';
import { PlaceOrderCommand } from './PlaceOrderCommand';

describe('PlaceOrder', () => {
  it('should place order successfully', () => {
    // Arrange
    const aggregate = new OrderAggregate();
    const command = new PlaceOrderCommand(
      'order-123',
      'customer-456',
      [{ productId: 'p1', quantity: 2, price: 10.00 }]
    );

    // Act
    (aggregate as any).handleCommand(command);

    // Assert
    const events = aggregate.getUncommittedEvents();
    expect(events).toHaveLength(1);
    expect(events[0].eventType).toBe('OrderPlaced');
    expect(events[0].customerId).toBe('customer-456');
  });

  it('should reject orders with no items', () => {
    const aggregate = new OrderAggregate();
    const command = new PlaceOrderCommand('order-123', 'customer-456', []);

    expect(() => (aggregate as any).handleCommand(command)).toThrow(
      'Order must have items'
    );
  });
});
```

### Integration Tests (With Event Store)

Test the full flow with real event store:

```typescript
import { MemoryEventStoreClient, RepositoryFactory } from '@event-sourcing-platform/typescript';
import { OrderAggregate } from './OrderAggregate';
import { PlaceOrderCommand } from './PlaceOrderCommand';

describe('PlaceOrder Integration', () => {
  let repository: Repository<OrderAggregate>;

  beforeAll(async () => {
    const client = new MemoryEventStoreClient();
    await client.connect();
    
    const factory = new RepositoryFactory(client);
    repository = factory.createRepository(
      () => new OrderAggregate(),
      'Order'
    );
  });

  it('should persist to event store and reload', async () => {
    // Create and save
    const aggregate = new OrderAggregate();
    const command = new PlaceOrderCommand('order-123', 'customer-456', [
      { productId: 'p1', quantity: 2, price: 10.00 }
    ]);
    (aggregate as any).handleCommand(command);
    await repository.save(aggregate);

    // Reload from event store
    const reloaded = await repository.load('order-123');
    expect(reloaded).toBeDefined();
    expect(reloaded!.getCustomerId()).toBe('customer-456');
  });
});
```

## Next Steps

- Review the complete VSA + Event Sourcing guide
- Check out working examples in the repository
- Explore the VSA CLI documentation
- [Examples](../examples/overview) - See it in action

---

More advanced topics coming soon!

