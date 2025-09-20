# 003-multiple-aggregates-ts — Multiple Aggregates Working Together

This example demonstrates how to work with multiple aggregate types in an event-sourced system. It shows how different aggregates can maintain their own state while referencing each other through IDs.

## What This Example Demonstrates

- **Multiple Aggregate Types**: Customer and Order aggregates with independent lifecycles
- **Cross-Aggregate References**: Orders reference Customers by ID (but don't load them)
- **Event Type Registration**: Proper registration of multiple event types for serialization
- **Repository Pattern**: Using separate repositories for different aggregate types
- **Business Logic**: Realistic business rules and state transitions
- **Event Sourcing Patterns**: Each aggregate maintains its state through events

## Domain Model

### Customer Aggregate
- **Events**: `CustomerRegistered`, `CustomerEmailUpdated`
- **Commands**: `register()`, `updateEmail()`
- **State**: email, name, status

### Order Aggregate
- **Events**: `OrderPlaced`, `OrderShipped`, `OrderCancelled`
- **Commands**: `place()`, `ship()`, `cancel()`
- **State**: customerId, items, totalAmount, status, trackingNumber

## Example Flow

1. Register a new customer
2. Update the customer's email address
3. Place an order for that customer
4. Ship the order with tracking information
5. Place a second order and cancel it
6. Reload all aggregates to verify final state

## Run

```bash
# Start dev infrastructure
make dev-start

# Start event store server (in separate terminal)
cd event-store
BACKEND=postgres DATABASE_URL=postgres://dev:dev@localhost:15648/dev cargo run -p eventstore-bin

# Run the example
pnpm --filter ./examples/003-multiple-aggregates-ts run start
```

Add `-- --memory` to run without the gRPC backend:

```bash
pnpm --filter ./examples/003-multiple-aggregates-ts run start -- --memory
```

## Key Learning Points

1. **Aggregate Independence**: Each aggregate manages its own state and events
2. **Reference by ID**: Aggregates reference each other by ID, not by loading the full aggregate
3. **Event Registration**: All event types must be registered with the EventSerializer
4. **Repository Separation**: Each aggregate type has its own repository
5. **Business Rules**: Aggregates enforce business rules (e.g., can't ship cancelled orders)
6. **State Reconstruction**: Aggregates rebuild their state from events when loaded

## Next Steps

This example sets the foundation for more advanced patterns:
- **004-cqrs-patterns**: Separating commands and queries with read models
- **005-projections**: Building read models from multiple aggregate streams
- **006-event-bus**: Cross-aggregate communication through events
