# TypeScript Event Sourcing SDK

High-level abstractions for building event-sourced systems on top of the platform event store.

## Aggregates

Aggregates extend `AutoDispatchAggregate` to encapsulate decision logic. Decorate state mutators with `@EventSourcingHandler` so events replay automatically.

```ts
import { AutoDispatchAggregate, EventSourcingHandler } from '@event-sourcing-platform/typescript';

class AccountAggregate extends AutoDispatchAggregate<AccountEvent> {
  private balance = 0;

  getAggregateType(): string {
    return 'Account';
  }

  deposit(amount: number): void {
    if (amount <= 0) throw new Error('amount must be positive');
    this.raiseEvent(new AccountCredited(amount));
  }

  @EventSourcingHandler('AccountCredited')
  private onCredited(event: AccountCredited): void {
    this.balance += event.amount;
  }
}
```

## Repositories

Create repositories through `RepositoryFactory`. They track optimistic concurrency and surface `ConcurrencyConflictError` when stale aggregates attempt to persist.

```ts
const client = new MemoryEventStoreClient();
const repository = new RepositoryFactory(client).createRepository(
  () => new AccountAggregate(),
  'Account'
);

const account = new AccountAggregate();
account.deposit(100);
await repository.save(account);
```

## gRPC Adapter

Use `GrpcEventStoreAdapter` with the generated gRPC client when talking to a live event store.

```ts
const adapter = new GrpcEventStoreAdapter({
  serverAddress: '127.0.0.1:50051',
  tenantId: 'tenant-a',
});

await adapter.appendEvents('Account-1', envelopeList, 0);
```

See `tests/` for end-to-end scenarios covering concurrency, stream existence, and gRPC round-trips.

