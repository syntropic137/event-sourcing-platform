# 004-cqrs-patterns-ts: CQRS Patterns Example

**Demonstrates:** Hexagonal Event-Sourced VSA Architecture with full CQRS separation

This example showcases **Command Query Responsibility Segregation (CQRS)** in a banking system, following the Hexagonal Event-Sourced VSA Architecture pattern. It demonstrates clear separation between command (write) and query (read) responsibilities with projections (read models).

## 🏗️ Architecture

This example perfectly demonstrates the **Hexagonal Event-Sourced VSA** pattern with CQRS:

```
004-cqrs-patterns-ts/
├── vsa.yaml                    # VSA configuration (validates architecture)
├── package.json
├── src/
│   ├── domain/                 # 🔵 DOMAIN LAYER (Hexagon Core)
│   │   ├── BankAccountAggregate.ts
│   │   ├── commands/           # Write model commands
│   │   │   ├── OpenAccountCommand.ts
│   │   │   ├── DepositMoneyCommand.ts
│   │   │   ├── WithdrawMoneyCommand.ts
│   │   │   └── CloseAccountCommand.ts
│   │   ├── queries/            # Read model queries
│   │   │   ├── GetAccountSummaryQuery.ts
│   │   │   ├── GetTransactionHistoryQuery.ts
│   │   │   └── GetAccountsByCustomerQuery.ts
│   │   └── events/             # Domain events
│   │       ├── AccountOpenedEvent.ts
│   │       ├── MoneyDepositedEvent.ts
│   │       ├── MoneyWithdrawnEvent.ts
│   │       └── AccountClosedEvent.ts
│   │
│   ├── infrastructure/         # 🟢 INFRASTRUCTURE (Application Services)
│   │   ├── CommandBus.ts       # Routes commands to aggregates
│   │   └── QueryBus.ts         # Routes queries to projections
│   │
│   ├── slices/                 # 🟡 ADAPTERS (Vertical Slices)
│   │   │
│   │   # COMMAND SLICES (Write Side)
│   │   ├── open-account/
│   │   │   └── OpenAccountCli.ts
│   │   ├── deposit-money/
│   │   │   └── DepositMoneyCli.ts
│   │   ├── withdraw-money/
│   │   │   └── WithdrawMoneyCli.ts
│   │   ├── close-account/
│   │   │   └── CloseAccountCli.ts
│   │   │
│   │   # QUERY SLICES (Read Side with Projections)
│   │   ├── get-account-summary/
│   │   │   ├── GetAccountSummaryCli.ts          # Query adapter
│   │   │   └── AccountSummaryProjection.ts      # Read model builder
│   │   ├── get-transaction-history/
│   │   │   ├── GetTransactionHistoryCli.ts      # Query adapter
│   │   │   └── TransactionHistoryProjection.ts  # Read model builder
│   │   └── get-accounts-by-customer/
│   │       └── GetAccountsByCustomerCli.ts      # Query adapter (uses shared projection)
│   │
│   └── main.ts                 # Entry point, wires everything together
```

### Key Architecture Principles

#### 🔵 Domain Layer (Hexagon Core)
- **`BankAccountAggregate`**: Contains ALL business logic for bank accounts
- **Commands**: Intent to change state (write model)
- **Queries**: Intent to read state (read model)
- **Events**: Immutable facts representing state changes
- **Zero external dependencies** (pure business logic)

#### 🟢 Infrastructure Layer (Application Services)
- **`CommandBus`**: Routes commands to the aggregate via repository
- **`QueryBus`**: Routes queries to projections (read models)
- **Shared infrastructure** used by all slices

#### 🟡 Adapter Layer (Vertical Slices)
- **Command Slices**: Thin CLI adapters that dispatch commands
- **Query Slices**: Thin CLI adapters + Projections (read model builders)
- **Each slice is isolated** and can be developed independently
- **No business logic** in slices (only translation)

## 🎯 CQRS Demonstration

### Write Side (Commands)
Commands modify state by going through:
1. **CLI Adapter** (e.g., `OpenAccountCli`) → Creates command object
2. **CommandBus** → Routes command to aggregate
3. **Aggregate** → Validates business rules and emits events
4. **Event Store** → Persists events

```typescript
// Command slice (adapter)
await openAccountCli.handle(accountId, customerId, "Checking", 1000);
// ↓
// CommandBus routes to BankAccountAggregate.openAccount()
// ↓
// Aggregate validates and emits AccountOpenedEvent
// ↓
// Event stored in event store
```

### Read Side (Queries)
Queries read optimized denormalized data:
1. **CLI Adapter** (e.g., `GetAccountSummaryCli`) → Creates query object
2. **QueryBus** → Routes query to projection
3. **Projection** → Returns pre-built read model
4. **CLI Adapter** → Formats and displays result

```typescript
// Query slice (adapter)
await getAccountSummaryCli.handle(accountId);
// ↓
// QueryBus routes to AccountSummaryProjection
// ↓
// Projection returns denormalized AccountSummary
// ↓
// CLI adapter formats and displays
```

### Projections (Read Models)
Projections build denormalized views from events:

```typescript
// AccountSummaryProjection processes events
accountSummaryProjection.processEvents(events);

// Events are transformed into optimized read models:
// AccountOpenedEvent → Creates AccountSummary
// MoneyDepositedEvent → Updates balance, increments transaction count
// MoneyWithdrawnEvent → Updates balance, increments transaction count
// AccountClosedEvent → Updates status
```

## 🚀 Running the Example

### Prerequisites
```bash
# From the workspace root
npm install
```

### Run with In-Memory Event Store (Easiest)
```bash
cd examples/004-cqrs-patterns-ts
npm run dev -- --memory
```

### Run with gRPC Event Store
```bash
# Start the event sourcing platform
make dev-start

# Run the example
cd examples/004-cqrs-patterns-ts
npm run dev
```

### Build and Run
```bash
npm run build
npm start
```

### Validate Architecture
```bash
# Validate that the architecture follows ADRs
npm run validate

# Or from the workspace root
vsa validate examples/004-cqrs-patterns-ts
```

## 📊 Expected Output

```
🏦 CQRS Patterns Example: Banking System
=========================================

📝 COMMAND SIDE - Processing Business Operations:
---------------------------------------------------
✅ Opened Checking account account-xxx with $1000
✅ Opened Savings account account-yyy with $5000
💰 Deposited $500 to account account-xxx
💸 Withdrew $200 from account account-xxx
💰 Deposited $1000 to account account-yyy

🔄 BUILDING READ MODELS - Processing Events into Projections:
-------------------------------------------------------------
📊 Built read models from 5 events

📖 QUERY SIDE - Reading Optimized Views:
-----------------------------------------

💳 Account Summary:
   Account ID: account-xxx
   Customer ID: customer-123
   Type: Checking
   Balance: $1300
   Status: Open
   Transactions: 2
   Last Activity: 2025-11-06T...

📋 Transaction History for account-xxx:
   1. +$500 - Salary deposit (Balance: $1500)
   2. -$200 - ATM withdrawal (Balance: $1300)

👤 Customer customer-123 has 2 accounts:
   Checking: $1300 (Open)
   Savings: $6000 (Open)

📝 ADDITIONAL COMMAND:
---------------------
🔒 Closed account account-yyy

💳 Account Summary:
   Account ID: account-yyy
   ...
   Status: Closed

🎉 CQRS Example completed successfully!
```

## 🧪 VSA Validation

This example is **ADR-compliant** and validated by the VSA CLI:

```bash
vsa validate
```

The VSA tool will check:
- ✅ Domain layer isolation (aggregates in `domain/`)
- ✅ Command organization (`domain/commands/`)
- ✅ Query organization (`domain/queries/`)
- ✅ Event organization with versioning (`domain/events/`)
- ✅ Vertical slice structure (`slices/` with CLI adapters)
- ✅ Infrastructure separation (`infrastructure/`)
- ✅ CQRS separation (command vs query slices)
- ✅ Dependency rules (adapters → infrastructure → domain)
- ✅ No cross-slice imports
- ✅ Event decorators present (`@Event`)

## 📚 What This Example Teaches

### 1. CQRS Pattern
- **Separate models** for reading and writing
- **Commands** change state (write model)
- **Queries** read state (read model)
- **Projections** build optimized views from events

### 2. Hexagonal Architecture
- **Domain** is isolated and pure
- **Infrastructure** provides shared services
- **Adapters** translate external protocols to domain operations

### 3. Event Sourcing
- **Events** are the source of truth
- **Aggregates** validate and emit events
- **Projections** derive read models from events
- **Event versioning** with `@Event` decorator

### 4. Vertical Slice Architecture
- **Slices are isolated** by feature
- **Each slice** is a thin adapter
- **No business logic** in slices
- **Parallel development** is possible

## 🔗 Related ADRs

- **ADR-004**: Command Handlers in Aggregates
- **ADR-005**: Hexagonal Architecture for Event Sourcing
- **ADR-006**: Domain Organization Pattern
- **ADR-007**: Event Versioning and Upcasters
- **ADR-008**: Vertical Slices as Hexagonal Adapters
- **ADR-009**: CQRS Pattern Implementation
- **ADR-010**: Decorator Patterns for Framework Integration

## 🔄 Comparison with Old Structure

### Before (Monolithic)
```
src/
└── index.ts  (600+ lines, everything mixed together)
```

### After (Hexagonal VSA)
```
src/
├── domain/           # Pure business logic
├── infrastructure/   # Shared services
├── slices/           # Isolated features
└── main.ts           # Wiring
```

**Benefits:**
- ✅ Clear separation of concerns
- ✅ Easy to test (pure domain logic)
- ✅ Easy to understand (each file has one responsibility)
- ✅ Easy to extend (add new slices without affecting others)
- ✅ VSA validated (architecture is enforced)
- ✅ ADR compliant (follows best practices)

## 📝 Next Steps

1. **Run the example** to see CQRS in action
2. **Explore the code** to understand the separation
3. **Run `vsa validate`** to see architecture validation
4. **Modify a slice** and see that others are unaffected
5. **Add a new command** (e.g., `TransferMoneyCommand`)
6. **Add a new query** (e.g., `GetAccountsByTypeQuery`)

---

**Need help?** Check the [Architecture Quick Start Guide](../../docs/HEXAGONAL-VSA-QUICK-START.md) or review the [ADRs](../../docs/adrs/).
