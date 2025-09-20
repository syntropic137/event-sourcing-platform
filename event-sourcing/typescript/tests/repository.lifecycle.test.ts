import { randomUUID } from 'crypto';

import { MemoryEventStoreClient, RepositoryFactory } from '../src';
import { OrderAggregate, OrderStatus } from './helpers/order-aggregate';

describe('Repository lifecycle with MemoryEventStoreClient', () => {
  it('saves, loads, and updates an aggregate', async () => {
    const client = new MemoryEventStoreClient();
    const repo = new RepositoryFactory(client).createRepository(
      () => new OrderAggregate(),
      'Order'
    );

    const orderId = 'order-1';
    const aggregate = new OrderAggregate();
    aggregate.submit(orderId, 'cust-1');
    await repo.save(aggregate);
    expect(aggregate.version).toBe(1);

    const loaded = await repo.load(orderId);
    expect(loaded).not.toBeNull();
    expect(loaded?.version).toBe(1);
    expect(loaded?.getStatus()).toBe(OrderStatus.Submitted);

    loaded?.cancel('requested');
    await repo.save(loaded!);
    const reloaded = await repo.load(orderId);
    expect(reloaded?.version).toBe(2);
    expect(reloaded?.getStatus()).toBe(OrderStatus.Cancelled);
  });

  it('rehydrates a fresh aggregate instance after cancellation using persisted JSON payloads', async () => {
    const client = new MemoryEventStoreClient();
    const repo = new RepositoryFactory(client).createRepository(
      () => new OrderAggregate(),
      'Order'
    );

    const orderId = `order-${randomUUID()}`;
    const aggregate = new OrderAggregate();
    aggregate.submit(orderId, 'cust-json');
    await repo.save(aggregate);

    aggregate.cancel('customer requested cancellation');
    await repo.save(aggregate);

    const events = await client.readEvents(`Order-${orderId}`);
    expect(events).toHaveLength(2);
    expect(events[0].event.toJson()).toMatchObject({
      orderId,
      customerId: 'cust-json',
    });
    expect(events[1].event.eventType).toBe('OrderCancelled');

    const replayed = new OrderAggregate();
    replayed.rehydrate(events);
    expect(replayed.getStatus()).toBe(OrderStatus.Cancelled);
  });
});
