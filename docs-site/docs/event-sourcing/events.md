# Events

Events are the heart of event sourcing - they represent immutable facts about what happened in your system.

## What is an Event?

An **Event** is a record of something that happened in the past. Events are:

- **Immutable** - Once created, they never change
- **Past tense** - Named with past-tense verbs (OrderPlaced, not PlaceOrder)
- **Factual** - They describe what happened, not what should happen
- **Rich** - They contain all the data needed to understand what occurred

## Event Structure

```typescript
interface DomainEvent {
  // Identity
  eventId: string;
  eventType: string;
  eventVersion: number;
  
  // Context
  aggregateId: string;
  aggregateType: string;
  aggregateNonce: number;
  
  // Metadata
  timestamp: Date;
  correlationId?: string;
  causationId?: string;
  actorId?: string;
  
  // Business data
  [key: string]: any;
}
```

## Event Examples

### Business Events
```typescript
interface OrderPlaced extends DomainEvent {
  eventType: 'OrderPlaced';
  orderId: string;
  customerId: string;
  items: LineItem[];
  totalAmount: number;
  placedAt: Date;
}

interface PaymentProcessed extends DomainEvent {
  eventType: 'PaymentProcessed';
  paymentId: string;
  orderId: string;
  amount: number;
  paymentMethod: string;
  processedAt: Date;
}

interface InventoryReserved extends DomainEvent {
  eventType: 'InventoryReserved';
  productId: string;
  quantity: number;
  reservationId: string;
  reservedAt: Date;
}
```

## Event Design Principles

### 1. Express Business Intent
Events should reflect the business domain, not technical operations.

```typescript
// ✅ Business-focused
interface CustomerRegistered {
  customerId: string;
  email: string;
  name: string;
  registeredAt: Date;
}

// ❌ Technical-focused
interface CustomerRecordInserted {
  tableId: number;
  columnValues: Record<string, any>;
  insertedAt: Date;
}
```

### 2. Include Sufficient Context
Events should contain all information needed to understand what happened.

```typescript
// ✅ Rich context
interface OrderShipped {
  orderId: string;
  customerId: string;
  shippingAddress: Address;
  trackingNumber: string;
  carrier: string;
  estimatedDelivery: Date;
  shippedAt: Date;
  shippedBy: string;
}

// ❌ Insufficient context
interface OrderShipped {
  orderId: string;
  shippedAt: Date;
}
```

### 3. Use Past Tense
Events describe what already happened.

```typescript
// ✅ Past tense
interface OrderPlaced { /* ... */ }
interface PaymentProcessed { /* ... */ }
interface InventoryReduced { /* ... */ }

// ❌ Present/future tense
interface PlaceOrder { /* ... */ }
interface ProcessPayment { /* ... */ }
interface ReduceInventory { /* ... */ }
```

## Event Versioning

As your system evolves, event schemas may need to change. Handle this with versioning:

```typescript
// Version 1
interface OrderPlacedV1 {
  eventType: 'OrderPlaced';
  eventVersion: 1;
  orderId: string;
  customerId: string;
  totalAmount: number;
}

// Version 2 - Added items array
interface OrderPlacedV2 {
  eventType: 'OrderPlaced';
  eventVersion: 2;
  orderId: string;
  customerId: string;
  items: LineItem[];
  totalAmount: number;
}

// Event upcasting
function upcastOrderPlaced(event: any): OrderPlacedV2 {
  if (event.eventVersion === 1) {
    return {
      ...event,
      eventVersion: 2,
      items: [] // Default for missing data
    };
  }
  return event;
}
```

## Event Metadata

### Correlation and Causation
Track how events relate to each other:

```typescript
interface EventMetadata {
  correlationId: string; // Groups related events (e.g., all events from one user request)
  causationId: string;   // The event that directly caused this event
}

// Example flow:
// 1. OrderPlaced (correlationId: req-123, causationId: null)
// 2. PaymentRequested (correlationId: req-123, causationId: orderPlaced.eventId)
// 3. PaymentProcessed (correlationId: req-123, causationId: paymentRequested.eventId)
```

### Actor Information
Track who or what caused the event:

```typescript
interface OrderPlaced {
  // ... other fields
  actorId: string;      // "user:john-doe" or "system:auto-reorder"
  actorType: 'user' | 'system' | 'api';
}
```

## Event Handlers

Events are processed by event handlers to update read models, trigger side effects, or communicate with other systems.

```typescript
class OrderProjectionHandler {
  @EventHandler(OrderPlaced)
  async handleOrderPlaced(event: OrderPlaced) {
    await this.orderReadModel.create({
      id: event.orderId,
      customerId: event.customerId,
      status: 'placed',
      totalAmount: event.totalAmount,
      placedAt: event.timestamp
    });
  }

  @EventHandler(OrderShipped)
  async handleOrderShipped(event: OrderShipped) {
    await this.orderReadModel.update(event.orderId, {
      status: 'shipped',
      trackingNumber: event.trackingNumber,
      shippedAt: event.timestamp
    });
  }
}
```

## Event Sourcing Patterns

### Event Enrichment
Add contextual information to events:

```typescript
class OrderAggregate {
  place(customerId: string, items: LineItem[]) {
    // Enrich event with calculated data
    const totalAmount = items.reduce((sum, item) => sum + item.price * item.quantity, 0);
    const itemCount = items.reduce((sum, item) => sum + item.quantity, 0);

    this.raiseEvent(new OrderPlaced({
      orderId: this.id,
      customerId,
      items,
      totalAmount,
      itemCount,
      placedAt: new Date()
    }));
  }
}
```

### Event Splitting
Break complex events into focused, single-purpose events:

```typescript
// ❌ Complex event
interface OrderProcessed {
  orderId: string;
  paymentProcessed: boolean;
  inventoryReserved: boolean;
  emailSent: boolean;
  shippingScheduled: boolean;
}

// ✅ Focused events
interface OrderPlaced { orderId: string; /* ... */ }
interface PaymentProcessed { orderId: string; /* ... */ }
interface InventoryReserved { orderId: string; /* ... */ }
interface OrderConfirmationSent { orderId: string; /* ... */ }
interface ShippingScheduled { orderId: string; /* ... */ }
```

## Testing Events

```typescript
describe('Order Events', () => {
  it('should create OrderPlaced event with correct data', () => {
    // Arrange
    const order = new OrderAggregate();
    const items = [{ productId: 'p1', quantity: 2, price: 10 }];

    // Act
    order.place('order-123', 'customer-456', items);

    // Assert
    const events = order.getUncommittedEvents();
    const orderPlaced = events[0] as OrderPlaced;
    
    expect(orderPlaced.eventType).toBe('OrderPlaced');
    expect(orderPlaced.orderId).toBe('order-123');
    expect(orderPlaced.customerId).toBe('customer-456');
    expect(orderPlaced.totalAmount).toBe(20);
    expect(orderPlaced.items).toEqual(items);
  });
});
```

## Best Practices

### 1. Make Events Self-Contained
Include all necessary data in the event itself:

```typescript
// ✅ Self-contained
interface ProductPriceChanged {
  productId: string;
  oldPrice: number;
  newPrice: number;
  changedBy: string;
  changedAt: Date;
  reason: string;
}

// ❌ Requires external lookups
interface ProductPriceChanged {
  productId: string;
  newPrice: number;
}
```

### 2. Use Meaningful Names
Event names should clearly communicate what happened:

```typescript
// ✅ Clear and specific
interface CustomerEmailAddressUpdated { /* ... */ }
interface OrderCancelledDueToPaymentFailure { /* ... */ }
interface InventoryReplenishedFromSupplier { /* ... */ }

// ❌ Vague or generic
interface CustomerUpdated { /* ... */ }
interface OrderChanged { /* ... */ }
interface InventoryModified { /* ... */ }
```

### 3. Keep Events Immutable
Never modify events after they're created:

```typescript
// ✅ Create new events for changes
const correctionEvent = new OrderTotalCorrected({
  orderId: originalEvent.orderId,
  originalAmount: originalEvent.totalAmount,
  correctedAmount: newAmount,
  reason: 'Tax calculation error'
});

// ❌ Don't modify existing events
// originalEvent.totalAmount = newAmount; // Never do this!
```

## Next Steps

- Learn about [Aggregates](./aggregates) that emit events
- Explore [Projections](./projections) that consume events
- See [Examples](./examples/) for event implementations in action
