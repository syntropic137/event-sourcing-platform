/**
 * Tests for readAll functionality in event store clients.
 */

import { MemoryEventStoreClient, RepositoryFactory } from '../src';
import { OrderAggregate } from './helpers/order-aggregate';

describe('MemoryEventStoreClient readAll', () => {
  let client: MemoryEventStoreClient;

  beforeEach(() => {
    // NODE_ENV should be 'test' in Jest
    client = new MemoryEventStoreClient();
  });

  it('returns events in global order', async () => {
    const repo = new RepositoryFactory(client).createRepository(
      () => new OrderAggregate(),
      'Order'
    );

    // Create events across different aggregates
    const order1 = new OrderAggregate();
    order1.submit('order-1', 'cust-1');
    await repo.save(order1);

    const order2 = new OrderAggregate();
    order2.submit('order-2', 'cust-2');
    await repo.save(order2);

    order1.cancel('test reason');
    await repo.save(order1);

    // Read all events
    const result = await client.readAll(0, 10, true);

    expect(result.events.length).toBe(3);
    expect(result.isEnd).toBe(true);

    // Verify global ordering
    const globalNonces = result.events.map((e) => e.metadata.globalNonce);
    expect(globalNonces).toEqual([1, 2, 3]);
  });

  it('supports pagination', async () => {
    const repo = new RepositoryFactory(client).createRepository(
      () => new OrderAggregate(),
      'Order'
    );

    // Create 5 orders
    for (let i = 0; i < 5; i++) {
      const order = new OrderAggregate();
      order.submit(`order-${i}`, `cust-${i}`);
      await repo.save(order);
    }

    // Read first page (2 events)
    const page1 = await client.readAll(0, 2, true);
    expect(page1.events.length).toBe(2);
    expect(page1.isEnd).toBe(false);

    // Read second page
    const page2 = await client.readAll(page1.nextFromGlobalNonce, 2, true);
    expect(page2.events.length).toBe(2);
    expect(page2.isEnd).toBe(false);

    // Read final page
    const page3 = await client.readAll(page2.nextFromGlobalNonce, 2, true);
    expect(page3.events.length).toBe(1);
    expect(page3.isEnd).toBe(true);
  });

  it('returns isEnd=true for empty store', async () => {
    const result = await client.readAll(0, 10, true);

    expect(result.events.length).toBe(0);
    expect(result.isEnd).toBe(true);
  });

  it('filters events by fromGlobalNonce', async () => {
    const repo = new RepositoryFactory(client).createRepository(
      () => new OrderAggregate(),
      'Order'
    );

    // Create 3 events
    const order = new OrderAggregate();
    order.submit('order-1', 'cust-1');
    await repo.save(order);

    const order2 = new OrderAggregate();
    order2.submit('order-2', 'cust-2');
    await repo.save(order2);

    const order3 = new OrderAggregate();
    order3.submit('order-3', 'cust-3');
    await repo.save(order3);

    // Read from global nonce 2 (should skip first event)
    const result = await client.readAll(2, 10, true);

    expect(result.events.length).toBe(2);
    expect(result.events[0].metadata.globalNonce).toBe(2);
    expect(result.events[1].metadata.globalNonce).toBe(3);
  });
});

describe('MemoryEventStoreClient environment guard', () => {
  it('throws error in non-test environment', async () => {
    const originalEnv = process.env.NODE_ENV;
    try {
      process.env.NODE_ENV = 'development';

      // Clear the module cache to force re-evaluation
      jest.resetModules();

      // Re-import to trigger the guard using dynamic import
      // eslint-disable-next-line @typescript-eslint/no-require-imports
      const mod = require('../src/client/event-store-memory');

      expect(() => new mod.MemoryEventStoreClient()).toThrow(
        /can only be used in test environment/
      );
    } finally {
      process.env.NODE_ENV = originalEnv;
      jest.resetModules();
    }
  });
});
