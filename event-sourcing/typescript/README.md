# TypeScript Event Sourcing SDK

High-level abstractions for building event-sourced systems with Event Sourcing, CQRS, and DDD patterns.

[![GitHub Package Registry](https://img.shields.io/badge/GitHub-Package%20Registry-blue)](https://github.com/NeuralEmpowerment/event-sourcing-platform/packages)
[![CI](https://github.com/NeuralEmpowerment/event-sourcing-platform/actions/workflows/test.yml/badge.svg)](https://github.com/NeuralEmpowerment/event-sourcing-platform/actions/workflows/test.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Installation

### Prerequisites

This package is published to GitHub Package Registry. You need to configure npm to use GitHub Packages for `@event-sourcing-platform` scoped packages.

Create or update `~/.npmrc`:

```bash
@event-sourcing-platform:registry=https://npm.pkg.github.com
//npm.pkg.github.com/:_authToken=YOUR_GITHUB_TOKEN
```

Replace `YOUR_GITHUB_TOKEN` with a GitHub Personal Access Token with `read:packages` permission.

[Create a token here](https://github.com/settings/tokens/new?scopes=read:packages&description=NPM%20GitHub%20Packages)

### Install the Package

```bash
npm install @event-sourcing-platform/typescript
# or
pnpm add @event-sourcing-platform/typescript
# or
yarn add @event-sourcing-platform/typescript
```

## Quick Start

```typescript
import {
  AggregateRoot,
  Aggregate,
  CommandHandler,
  EventSourcingHandler,
  BaseDomainEvent,
  RepositoryFactory,
  MemoryEventStoreClient
} from '@event-sourcing-platform/typescript';

// 1. Define your command
class DepositMoneyCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly amount: number
  ) {}
}

// 2. Define your event
class MoneyDeposited extends BaseDomainEvent {
  constructor(
    public readonly accountId: string,
    public readonly amount: number
  ) {
    super();
  }
}

// 3. Create your aggregate
@Aggregate('BankAccount')
class BankAccountAggregate extends AggregateRoot<MoneyDeposited> {
  private balance = 0;

  getAggregateType(): string {
    return 'BankAccount';
  }

  @CommandHandler('DepositMoneyCommand')
  depositMoney(command: DepositMoneyCommand): void {
    if (command.amount <= 0) {
      throw new Error('Amount must be positive');
    }
    if (!this.id) {
      this.initialize(command.aggregateId);
    }
    this.apply(new MoneyDeposited(command.aggregateId, command.amount));
  }

  @EventSourcingHandler('MoneyDeposited')
  private onMoneyDeposited(event: MoneyDeposited): void {
    this.balance += event.amount;
  }

  getBalance(): number {
    return this.balance;
  }
}

// 4. Use the aggregate with a repository
async function main() {
  const client = new MemoryEventStoreClient();
  await client.connect();

  const repository = new RepositoryFactory(client).createRepository(
    () => new BankAccountAggregate(),
    'BankAccount'
  );

  const account = new BankAccountAggregate();
  const command = new DepositMoneyCommand('account-1', 100);
  (account as any).handleCommand(command);

  await repository.save(account);
  console.log('Account balance:', account.getBalance()); // 100
}
```

## Features

- ✅ **Command Handling** - `@CommandHandler` decorator for business logic
- ✅ **Event Sourcing** - `@EventSourcingHandler` decorator for state updates
- ✅ **Aggregates** - Domain-driven design aggregate pattern
- ✅ **Repository** - Persistence abstraction with optimistic concurrency
- ✅ **Multiple Backends** - In-memory and gRPC event store support
- ✅ **TypeScript First** - Full type safety and IntelliSense
- ✅ **Decorators** - Clean, declarative API

## Core Concepts

### Commands

Commands represent intentions to change state. They must be classes with an `aggregateId` property:

```ts
export class DepositMoneyCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly amount: number
  ) {}
}
```

### Aggregates with Command Handlers

Aggregates encapsulate business logic and state. Use `@CommandHandler` for business logic and validation, `@EventSourcingHandler` for state updates:

```ts
import {
  AggregateRoot,
  Aggregate,
  CommandHandler,
  EventSourcingHandler
} from '@event-sourcing-platform/typescript';

@Aggregate('Account')
class AccountAggregate extends AggregateRoot<AccountEvent> {
  private balance = 0;

  getAggregateType(): string {
    return 'Account';
  }

  @CommandHandler('DepositMoneyCommand')
  deposit(command: DepositMoneyCommand): void {
    // Business logic and validation
    if (command.amount <= 0) {
      throw new Error('Amount must be positive');
    }
    
    // Initialize for new aggregates
    if (!this.id) {
      this.initialize(command.aggregateId);
    }
    
    // Emit event
    this.apply(new AccountCredited(command.aggregateId, command.amount));
  }

  @EventSourcingHandler('AccountCredited')
  private onCredited(event: AccountCredited): void {
    // State updates only - no business logic
    this.balance += event.amount;
  }

  getBalance(): number {
    return this.balance;
  }
}
```

### Repositories

Repositories handle loading and saving aggregates with optimistic concurrency control:

```ts
import { RepositoryFactory, MemoryEventStoreClient } from '@event-sourcing-platform/typescript';

const client = new MemoryEventStoreClient();
await client.connect();

const repository = new RepositoryFactory(client).createRepository(
  () => new AccountAggregate(),
  'Account'
);

// Create new aggregate
const account = new AccountAggregate();
const command = new DepositMoneyCommand('account-1', 100);
(account as any).handleCommand(command);
await repository.save(account);

// Load existing aggregate
const loaded = await repository.load('account-1');
if (loaded) {
  console.log('Balance:', loaded.getBalance());
}
```

### Event Store Backends

#### In-Memory (for testing)

```ts
import { MemoryEventStoreClient } from '@event-sourcing-platform/typescript';

const client = new MemoryEventStoreClient();
await client.connect();
```

#### gRPC (for production)

```ts
import {
  EventStoreClientFactory
} from '@event-sourcing-platform/typescript';

const client = EventStoreClientFactory.createGrpcClient({
  serverAddress: '127.0.0.1:50051',
  tenantId: 'my-tenant'
});
await client.connect();
```

## Examples

See the [examples directory](../../examples/) for complete working examples:

- **[001-basic-store-ts](../../examples/001-basic-store-ts)** - Basic event store usage and aggregate patterns
- **[002-simple-aggregate-ts](../../examples/002-simple-aggregate-ts)** - Simple aggregate with commands
- **[003-multiple-aggregates-ts](../../examples/003-multiple-aggregates-ts)** - Multiple aggregates and repositories
- **[004-cqrs-patterns-ts](../../examples/004-cqrs-patterns-ts)** - CQRS with projections
- **[010-observability-ts](../../examples/010-observability-ts)** - Logging and observability patterns

## Documentation

- **[Migration Guide](../../MIGRATION-GUIDE.md)** - Migrate from old patterns
- **[VSA + Event Sourcing Guide](../../docs-site/docs/guides/vsa-event-sourcing-guide.md)** - Complete integration guide
- **[ADR-004](../../docs/adrs/ADR-004-command-handlers-in-aggregates.md)** - Architecture decision record

## Testing

```bash
# Run tests
pnpm test

# Run tests in watch mode
pnpm test:watch

# Run linter
pnpm lint

# Build
pnpm build
```

## Contributing

See [CONTRIBUTING.md](../../.github/CONTRIBUTING.md) for development guidelines.

## License

MIT - See [LICENSE](../../LICENSE) for details.

## Support

- 📖 [Documentation](../../docs-site/)
- 💬 [GitHub Discussions](https://github.com/NeuralEmpowerment/event-sourcing-platform/discussions)
- 🐛 [Issue Tracker](https://github.com/NeuralEmpowerment/event-sourcing-platform/issues)

