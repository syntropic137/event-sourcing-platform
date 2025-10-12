# Projections

Projections are read models built from events - they transform your event stream into queryable data structures optimized for specific use cases.

## What is a Projection?

A **Projection** is a read-only view of your data, built by processing events in sequence. Think of it as a "materialized view" that answers specific questions about your domain.

```typescript
class OrderSummaryProjection {
  private orders = new Map<string, OrderSummary>();

  @EventHandler(OrderPlaced)
  handleOrderPlaced(event: OrderPlaced) {
    this.orders.set(event.orderId, {
      id: event.orderId,
      customerId: event.customerId,
      status: 'placed',
      totalAmount: event.totalAmount,
      itemCount: event.items.length,
      placedAt: event.timestamp
    });
  }

  @EventHandler(OrderShipped)
  handleOrderShipped(event: OrderShipped) {
    const order = this.orders.get(event.orderId);
    if (order) {
      order.status = 'shipped';
      order.shippedAt = event.timestamp;
      order.trackingNumber = event.trackingNumber;
    }
  }

  getOrdersByCustomer(customerId: string): OrderSummary[] {
    return Array.from(this.orders.values())
      .filter(order => order.customerId === customerId);
  }
}
```

## Types of Projections

### 1. List Projections
Simple collections of data:

```typescript
class ProductCatalogProjection {
  private products = new Map<string, Product>();

  @EventHandler(ProductCreated)
  handleProductCreated(event: ProductCreated) {
    this.products.set(event.productId, {
      id: event.productId,
      name: event.name,
      price: event.price,
      category: event.category,
      createdAt: event.timestamp
    });
  }

  getAllProducts(): Product[] {
    return Array.from(this.products.values());
  }

  getProductsByCategory(category: string): Product[] {
    return this.getAllProducts()
      .filter(product => product.category === category);
  }
}
```

### 2. Aggregated Projections
Statistical summaries and analytics:

```typescript
class SalesAnalyticsProjection {
  private dailySales = new Map<string, DailySales>();
  private productSales = new Map<string, ProductSales>();

  @EventHandler(OrderPlaced)
  handleOrderPlaced(event: OrderPlaced) {
    const date = event.timestamp.toISOString().split('T')[0];
    
    // Update daily sales
    const dailySales = this.dailySales.get(date) || {
      date,
      totalRevenue: 0,
      orderCount: 0,
      averageOrderValue: 0
    };
    
    dailySales.totalRevenue += event.totalAmount;
    dailySales.orderCount += 1;
    dailySales.averageOrderValue = dailySales.totalRevenue / dailySales.orderCount;
    
    this.dailySales.set(date, dailySales);

    // Update product sales
    event.items.forEach(item => {
      const productSales = this.productSales.get(item.productId) || {
        productId: item.productId,
        totalQuantitySold: 0,
        totalRevenue: 0
      };
      
      productSales.totalQuantitySold += item.quantity;
      productSales.totalRevenue += item.price * item.quantity;
      
      this.productSales.set(item.productId, productSales);
    });
  }

  getDailySales(date: string): DailySales | undefined {
    return this.dailySales.get(date);
  }

  getTopSellingProducts(limit: number = 10): ProductSales[] {
    return Array.from(this.productSales.values())
      .sort((a, b) => b.totalQuantitySold - a.totalQuantitySold)
      .slice(0, limit);
  }
}
```

### 3. Denormalized Projections
Optimized for specific queries:

```typescript
class CustomerOrderHistoryProjection {
  private customerOrders = new Map<string, CustomerOrderHistory>();

  @EventHandler(OrderPlaced)
  handleOrderPlaced(event: OrderPlaced) {
    const history = this.customerOrders.get(event.customerId) || {
      customerId: event.customerId,
      orders: [],
      totalSpent: 0,
      orderCount: 0,
      firstOrderDate: event.timestamp,
      lastOrderDate: event.timestamp
    };

    history.orders.push({
      orderId: event.orderId,
      totalAmount: event.totalAmount,
      placedAt: event.timestamp,
      status: 'placed'
    });

    history.totalSpent += event.totalAmount;
    history.orderCount += 1;
    history.lastOrderDate = event.timestamp;

    this.customerOrders.set(event.customerId, history);
  }

  getCustomerHistory(customerId: string): CustomerOrderHistory | undefined {
    return this.customerOrders.get(customerId);
  }

  getTopCustomers(limit: number = 10): CustomerOrderHistory[] {
    return Array.from(this.customerOrders.values())
      .sort((a, b) => b.totalSpent - a.totalSpent)
      .slice(0, limit);
  }
}
```

## Projection Patterns

### 1. Event Filtering
Only process relevant events:

```typescript
class InventoryProjection {
  private inventory = new Map<string, InventoryLevel>();

  @EventHandler(ProductCreated)
  @EventHandler(StockReceived)
  @EventHandler(StockReserved)
  @EventHandler(StockReleased)
  handleInventoryEvent(event: DomainEvent) {
    switch (event.eventType) {
      case 'ProductCreated':
        this.handleProductCreated(event as ProductCreated);
        break;
      case 'StockReceived':
        this.handleStockReceived(event as StockReceived);
        break;
      // ... other cases
    }
  }

  private handleStockReceived(event: StockReceived) {
    const current = this.inventory.get(event.productId) || {
      productId: event.productId,
      available: 0,
      reserved: 0,
      total: 0
    };

    current.available += event.quantity;
    current.total += event.quantity;

    this.inventory.set(event.productId, current);
  }
}
```

### 2. Event Transformation
Transform events into different formats:

```typescript
class NotificationProjection {
  private notifications: Notification[] = [];

  @EventHandler(OrderPlaced)
  handleOrderPlaced(event: OrderPlaced) {
    this.notifications.push({
      id: generateId(),
      type: 'order_confirmation',
      recipientId: event.customerId,
      title: 'Order Confirmed',
      message: `Your order #${event.orderId} for $${event.totalAmount} has been placed.`,
      createdAt: event.timestamp,
      read: false
    });
  }

  @EventHandler(OrderShipped)
  handleOrderShipped(event: OrderShipped) {
    this.notifications.push({
      id: generateId(),
      type: 'shipping_update',
      recipientId: event.customerId,
      title: 'Order Shipped',
      message: `Your order has shipped! Tracking: ${event.trackingNumber}`,
      createdAt: event.timestamp,
      read: false
    });
  }
}
```

### 3. Multi-Stream Projections
Combine events from multiple aggregates:

```typescript
class OrderFulfillmentProjection {
  private fulfillments = new Map<string, OrderFulfillment>();

  @EventHandler(OrderPlaced)
  handleOrderPlaced(event: OrderPlaced) {
    this.fulfillments.set(event.orderId, {
      orderId: event.orderId,
      customerId: event.customerId,
      orderStatus: 'placed',
      paymentStatus: 'pending',
      shippingStatus: 'pending',
      totalAmount: event.totalAmount,
      placedAt: event.timestamp
    });
  }

  @EventHandler(PaymentProcessed)
  handlePaymentProcessed(event: PaymentProcessed) {
    const fulfillment = this.fulfillments.get(event.orderId);
    if (fulfillment) {
      fulfillment.paymentStatus = 'completed';
      fulfillment.paidAt = event.timestamp;
    }
  }

  @EventHandler(OrderShipped)
  handleOrderShipped(event: OrderShipped) {
    const fulfillment = this.fulfillments.get(event.orderId);
    if (fulfillment) {
      fulfillment.shippingStatus = 'shipped';
      fulfillment.shippedAt = event.timestamp;
      fulfillment.trackingNumber = event.trackingNumber;
    }
  }
}
```

## Projection Management

### Building Projections
```typescript
class ProjectionManager {
  private projections: Projection[] = [];

  register(projection: Projection) {
    this.projections.push(projection);
  }

  async rebuild(projectionName?: string) {
    const targetProjections = projectionName 
      ? this.projections.filter(p => p.getName() === projectionName)
      : this.projections;

    for (const projection of targetProjections) {
      await projection.reset();
      
      // Replay all events
      const events = await this.eventStore.readAllEvents();
      for (const event of events) {
        await projection.handleEvent(event);
      }
    }
  }

  async processEvent(event: DomainEvent) {
    for (const projection of this.projections) {
      try {
        await projection.handleEvent(event);
      } catch (error) {
        console.error(`Error in projection ${projection.getName()}:`, error);
        // Handle projection errors (retry, dead letter, etc.)
      }
    }
  }
}
```

### Projection Snapshots
For performance, save projection state periodically:

```typescript
class SnapshotableProjection {
  private lastSnapshotVersion = 0;
  private readonly snapshotInterval = 1000; // Every 1000 events

  async handleEvent(event: DomainEvent) {
    await this.processEvent(event);
    
    if (event.globalNonce - this.lastSnapshotVersion >= this.snapshotInterval) {
      await this.saveSnapshot(event.globalNonce);
      this.lastSnapshotVersion = event.globalNonce;
    }
  }

  async saveSnapshot(version: number) {
    const snapshot = {
      projectionName: this.getName(),
      version,
      data: this.getState(),
      createdAt: new Date()
    };
    
    await this.snapshotStore.save(snapshot);
  }

  async loadFromSnapshot(): Promise<number> {
    const snapshot = await this.snapshotStore.getLatest(this.getName());
    if (snapshot) {
      this.setState(snapshot.data);
      return snapshot.version;
    }
    return 0;
  }
}
```

## Testing Projections

```typescript
describe('OrderSummaryProjection', () => {
  let projection: OrderSummaryProjection;

  beforeEach(() => {
    projection = new OrderSummaryProjection();
  });

  it('should create order summary when order is placed', async () => {
    // Arrange
    const event = new OrderPlaced({
      orderId: 'order-123',
      customerId: 'customer-456',
      totalAmount: 100,
      items: [{ productId: 'p1', quantity: 2, price: 50 }],
      timestamp: new Date()
    });

    // Act
    await projection.handleEvent(event);

    // Assert
    const orders = projection.getOrdersByCustomer('customer-456');
    expect(orders).toHaveLength(1);
    expect(orders[0].id).toBe('order-123');
    expect(orders[0].status).toBe('placed');
    expect(orders[0].totalAmount).toBe(100);
  });

  it('should update order status when shipped', async () => {
    // Arrange
    const placedEvent = new OrderPlaced({ /* ... */ });
    const shippedEvent = new OrderShipped({
      orderId: 'order-123',
      trackingNumber: 'TRACK123',
      timestamp: new Date()
    });

    // Act
    await projection.handleEvent(placedEvent);
    await projection.handleEvent(shippedEvent);

    // Assert
    const orders = projection.getOrdersByCustomer('customer-456');
    expect(orders[0].status).toBe('shipped');
    expect(orders[0].trackingNumber).toBe('TRACK123');
  });
});
```

## Best Practices

### 1. Single Responsibility
Each projection should answer specific questions:

```typescript
// ✅ Focused projection
class CustomerLifetimeValueProjection {
  // Only handles customer value calculations
}

// ❌ Mixed responsibilities
class CustomerEverythingProjection {
  // Handles orders, payments, preferences, analytics, etc.
}
```

### 2. Idempotent Processing
Handle duplicate events gracefully:

```typescript
class OrderProjection {
  private processedEvents = new Set<string>();

  async handleEvent(event: DomainEvent) {
    if (this.processedEvents.has(event.eventId)) {
      return; // Already processed
    }

    await this.processEvent(event);
    this.processedEvents.add(event.eventId);
  }
}
```

### 3. Error Handling
Implement robust error handling:

```typescript
class ResilientProjection {
  async handleEvent(event: DomainEvent) {
    try {
      await this.processEvent(event);
    } catch (error) {
      if (this.isRetryableError(error)) {
        await this.scheduleRetry(event, error);
      } else {
        await this.sendToDeadLetter(event, error);
      }
    }
  }
}
```

## Performance Considerations

### 1. Batch Processing
Process multiple events together:

```typescript
class BatchProjection {
  private eventBatch: DomainEvent[] = [];
  private readonly batchSize = 100;

  async handleEvent(event: DomainEvent) {
    this.eventBatch.push(event);
    
    if (this.eventBatch.length >= this.batchSize) {
      await this.processBatch();
    }
  }

  private async processBatch() {
    // Process all events in batch
    for (const event of this.eventBatch) {
      await this.processEvent(event);
    }
    
    // Commit changes
    await this.commit();
    this.eventBatch = [];
  }
}
```

### 2. Indexing
Create appropriate indexes for queries:

```typescript
class IndexedProjection {
  private orders = new Map<string, Order>();
  private customerIndex = new Map<string, string[]>(); // customerId -> orderIds
  private dateIndex = new Map<string, string[]>(); // date -> orderIds

  @EventHandler(OrderPlaced)
  handleOrderPlaced(event: OrderPlaced) {
    // Store order
    this.orders.set(event.orderId, { /* ... */ });
    
    // Update indexes
    this.addToIndex(this.customerIndex, event.customerId, event.orderId);
    this.addToIndex(this.dateIndex, event.timestamp.toDateString(), event.orderId);
  }
}
```

## Next Steps

- Learn about Event Bus for cross-aggregate communication
- Explore CQRS patterns that leverage projections
- See [Examples](./examples/) for projection implementations in action
