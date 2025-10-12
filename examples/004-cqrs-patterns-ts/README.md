# 004-cqrs-patterns-ts — Command/Query Responsibility Segregation

This example demonstrates CQRS (Command/Query Responsibility Segregation) patterns in an event-sourced banking system. It shows how to separate write operations (commands) from read operations (queries) using different models optimized for each purpose.

## What This Example Demonstrates

- **CQRS Architecture**: Clear separation between command and query sides
- **Command Handlers**: Dedicated handlers for processing business operations
- **Query Handlers**: Optimized handlers for reading denormalized data
- **Read Models**: Projections built from events for efficient querying
- **Event-Driven Projections**: Building read models by processing event streams
- **Multiple Views**: Different read models for different query needs

## Architecture

### Command Side (Write)
- **Commands**: Structured requests for business operations
- **Command Handlers**: Process commands and coordinate with aggregates
- **Aggregates**: Enforce business rules and emit events
- **Events**: Record what happened in the system

### Query Side (Read)
- **Read Models**: Denormalized views optimized for specific queries
- **Query Handlers**: Process queries against read models
- **Projections**: Event processors that build and maintain read models

## Domain Model: Banking System

### Commands
- `OpenAccountCommand`: Open a new bank account
- `DepositMoneyCommand`: Deposit money to an account
- `WithdrawMoneyCommand`: Withdraw money from an account
- `CloseAccountCommand`: Close an account

### Events
- `AccountOpened`: Account was created
- `MoneyDeposited`: Money was added to account
- `MoneyWithdrawn`: Money was removed from account
- `AccountClosed`: Account was closed

### Read Models
- `AccountSummary`: Account overview with balance and transaction count
- `TransactionHistory`: Detailed transaction log with running balances

## Example Flow

1. **Command Processing**: Open accounts and perform transactions
2. **Event Storage**: All operations are stored as events
3. **Projection Building**: Read models are built from event streams
4. **Query Processing**: Different views are queried efficiently
5. **Real-time Updates**: Read models are updated as new events occur

## Run

```bash
# Start dev infrastructure
make dev-start

# Start event store server (in separate terminal)
cd event-store
BACKEND=postgres DATABASE_URL=postgres://dev:dev@localhost:15648/dev cargo run -p eventstore-bin

# Run the example
pnpm --filter ./examples/004-cqrs-patterns-ts run start
```

Add `-- --memory` to run without the gRPC backend:

```bash
pnpm --filter ./examples/004-cqrs-patterns-ts run start -- --memory
```

## Key Learning Points

1. **Separation of Concerns**: Commands and queries have different optimization needs
2. **Write Optimization**: Command side optimized for consistency and business rules
3. **Read Optimization**: Query side optimized for fast, flexible data access
4. **Event-Driven Projections**: Read models are built and maintained from events
5. **Multiple Views**: Same data can be projected into different read models
6. **Eventual Consistency**: Read models are eventually consistent with write side

## CQRS Benefits Demonstrated

- **Scalability**: Read and write sides can be scaled independently
- **Performance**: Queries run against optimized, denormalized data
- **Flexibility**: Multiple read models for different use cases
- **Maintainability**: Clear separation of read and write concerns
- **Evolution**: Read models can be rebuilt from events as requirements change

## Next Steps

This example sets the foundation for:
- **005-projections**: Advanced projection patterns and techniques
- **006-event-bus**: Cross-aggregate communication through events
- **007-ecommerce-complete**: Full system with multiple bounded contexts
