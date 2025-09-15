import {
  AutoDispatchAggregate,
  BaseDomainEvent,
  EventSourcingHandler,
  EventSerializer,
  RepositoryFactory,
  EventStoreClientFactory,
} from '@event-sourcing-platform/typescript';

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

async function main() {
  // Register events for deserialization via gRPC adapter
  EventSerializer.registerEvent('OrderSubmitted', OrderSubmitted as any);
  EventSerializer.registerEvent('OrderCancelled', OrderCancelled as any);

  const client = EventStoreClientFactory.createGrpcClient({ serverAddress: 'localhost:50051' });
  await client.connect();

  const repo = new RepositoryFactory(client as any).createRepository(() => new OrderAggregate(), 'Order');

  const orderId = 'order-' + Math.random().toString(36).slice(2);
  const agg = new OrderAggregate();
  agg.submit(orderId, 'customer-xyz');
  await repo.save(agg);
  console.log('Saved order v', agg.version);

  const loaded = await repo.load(orderId);
  console.log('Loaded v', loaded?.version, 'status', (loaded as any)?.getStatus());

  loaded!.cancel('customer request');
  await repo.save(loaded!);

  const reloaded = await repo.load(orderId);
  console.log('Reloaded v', reloaded?.version, 'status', (reloaded as any)?.getStatus());
}

if (require.main === module) {
  main().catch((e) => { console.error(e); process.exit(1); });
}

