import { describe, it, expect } from '@jest/globals';
import {
  AutoDispatchAggregate,
  BaseDomainEvent,
  RepositoryFactory,
  MemoryEventStoreClient,
  EventSourcingHandler,
} from '../src';

class OrderSubmitted extends BaseDomainEvent {
  readonly eventType = 'OrderSubmitted' as const;
  readonly schemaVersion = 1 as const;
  constructor(public orderId: string, public customerId: string) { super(); }
}

class OrderCancelled extends BaseDomainEvent {
  readonly eventType = 'OrderCancelled' as const;
  readonly schemaVersion = 1 as const;
  constructor(public reason: string) { super(); }
}

type OrderEvent = OrderSubmitted | OrderCancelled;

enum OrderStatus { New = 'New', Submitted = 'Submitted', Cancelled = 'Cancelled' }

class OrderAggregate extends AutoDispatchAggregate<OrderEvent> {
  private status: OrderStatus = OrderStatus.New;
  getAggregateType(): string { return 'Order'; }

  submit(orderId: string, customerId: string) {
    if (!this.id) this.initialize(orderId);
    if (this.status !== OrderStatus.New) throw new Error('Invalid state');
    this.raiseEvent(new OrderSubmitted(orderId, customerId));
  }

  cancel(reason: string) {
    if (this.status !== OrderStatus.Submitted) throw new Error('Invalid state');
    this.raiseEvent(new OrderCancelled(reason));
  }

  // Handlers
  @EventSourcingHandler('OrderSubmitted')
  onSubmitted(e: OrderSubmitted) {
    if (!this.id) this.initialize(e.orderId);
    this.status = OrderStatus.Submitted;
  }

  @EventSourcingHandler('OrderCancelled')
  onCancelled(_: OrderCancelled) {
    this.status = OrderStatus.Cancelled;
  }

  getStatus() { return this.status; }
}

describe('Repository lifecycle with MemoryEventStoreClient', () => {
  it('saves, loads, and updates an aggregate', async () => {
    const client = new MemoryEventStoreClient();
    const repo = new RepositoryFactory(client as any).createRepository(() => new OrderAggregate(), 'Order');

    const orderId = 'order-1';
    const agg = new OrderAggregate();
    agg.submit(orderId, 'cust-1');
    await repo.save(agg);
    expect(agg.version).toBe(1);

    const loaded = await repo.load(orderId);
    expect(loaded).not.toBeNull();
    expect(loaded!.version).toBe(1);

    loaded!.cancel('requested');
    await repo.save(loaded!);
    const reloaded = await repo.load(orderId);
    expect(reloaded!.version).toBe(2);
  });
});
