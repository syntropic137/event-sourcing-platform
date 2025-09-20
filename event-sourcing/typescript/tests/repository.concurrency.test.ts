import { randomUUID } from 'crypto';

import { ConcurrencyConflictError, MemoryEventStoreClient, RepositoryFactory } from '../src';
import { OrderAggregate, OrderStatus } from './helpers/order-aggregate';

describe('Repository concurrency semantics', () => {
  it('throws ConcurrencyConflictError when saving a stale aggregate', async () => {
    const client = new MemoryEventStoreClient();
    const repository = new RepositoryFactory(client).createRepository(
      () => new OrderAggregate(),
      'Order'
    );

    const orderId = `order-${randomUUID()}`;

    const created = new OrderAggregate();
    created.submit(orderId, 'cust-1');
    await repository.save(created);

    const stale = (await repository.load(orderId))!;
    const concurrent = (await repository.load(orderId))!;

    concurrent.cancel('first writer');
    await repository.save(concurrent);

    stale.cancel('stale writer');
    await expect(repository.save(stale)).rejects.toBeInstanceOf(ConcurrencyConflictError);
  });

  it('streamExists reflects whether events have been written', async () => {
    const client = new MemoryEventStoreClient();

    const existsBefore = await client.streamExists('Order-nope');
    expect(existsBefore).toBe(false);

    const repository = new RepositoryFactory(client).createRepository(
      () => new OrderAggregate(),
      'Order'
    );

    const orderId = `order-${randomUUID()}`;
    const aggregate = new OrderAggregate();
    aggregate.submit(orderId, 'cust-2');
    await repository.save(aggregate);

    const existsAfter = await client.streamExists(`Order-${orderId}`);
    expect(existsAfter).toBe(true);
  });

  it('round-trips payloads and metadata through the memory client', async () => {
    const client = new MemoryEventStoreClient();
    const repository = new RepositoryFactory(client).createRepository(
      () => new OrderAggregate(),
      'Order'
    );

    const orderId = `order-${randomUUID()}`;
    const aggregate = new OrderAggregate();
    aggregate.submit(orderId, 'cust-hash');
    await repository.save(aggregate);

    const streamEvents = await client.readEvents(`Order-${orderId}`);
    expect(streamEvents).toHaveLength(1);
    const [envelope] = streamEvents;
    expect(envelope.metadata.aggregateId).toBe(orderId);
    expect(envelope.metadata.contentType).toBe('application/json');

    const loaded = await repository.load(orderId);
    expect(loaded).not.toBeNull();
    expect(loaded?.getStatus()).toBe(OrderStatus.Submitted);
  });
});
