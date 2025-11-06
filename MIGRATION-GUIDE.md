# Migration Guide: Adopting the @CommandHandler Pattern

**Version:** 1.0  
**Date:** November 2025  
**Status:** Stable

## Overview

This guide helps you migrate from the old command handler pattern (separate handler classes) to the new `@CommandHandler` decorator pattern (handlers on aggregates).

### What Changed?

**Old Pattern** (Deprecated):
- Commands as interfaces
- Separate handler classes
- Handlers call aggregate methods
- More files per feature

**New Pattern** (Recommended):
- Commands as classes with `aggregateId`
- `@CommandHandler` decorators on aggregate methods
- Direct command dispatching to aggregates
- Fewer files, clearer intent

### Why Migrate?

✅ **Simpler** - Fewer files and concepts  
✅ **Clearer** - Business logic lives with the aggregate  
✅ **Type-safe** - Better TypeScript support  
✅ **Consistent** - Matches DDD principles  
✅ **Testable** - Direct aggregate testing  
✅ **Maintainable** - All related code in one place  

---

## Migration Steps

### Step 1: Update Commands (Interfaces → Classes)

**Before:**
```typescript
// PlaceOrderCommand.ts
export interface PlaceOrderCommand {
  orderId: string;
  customerId: string;
  items: Item[];
}
```

**After:**
```typescript
// PlaceOrderCommand.ts
export class PlaceOrderCommand {
  constructor(
    public readonly aggregateId: string,  // ← Required!
    public readonly customerId: string,
    public readonly items: Item[]
  ) {}
}
```

**Key Changes:**
- Change `interface` to `class`
- Add `aggregateId` property (must be first parameter)
- Use `readonly` modifiers
- Add constructor

---

### Step 2: Add Decorators to Aggregate

**Before:**
```typescript
// OrderAggregate.ts
class OrderAggregate extends AggregateRoot<OrderEvent> {
  private status = 'pending';

  placeOrder(items: Item[]): void {
    this.raiseEvent(new OrderPlaced(items));
  }

  @EventSourcingHandler('OrderPlaced')
  private onPlaced(event: OrderPlaced): void {
    this.status = 'placed';
  }
}
```

**After:**
```typescript
// OrderAggregate.ts
@Aggregate('Order')  // ← Add @Aggregate decorator
class OrderAggregate extends AggregateRoot<OrderEvent> {
  private status = 'pending';

  // ← Add @CommandHandler decorator
  @CommandHandler('PlaceOrderCommand')
  placeOrder(command: PlaceOrderCommand): void {
    // 1. Validate business rules
    if (!command.items.length) {
      throw new Error('Order must have items');
    }
    
    // 2. Initialize (for new aggregates)
    if (!this.id) {
      this.initialize(command.aggregateId);
    }
    
    // 3. Emit event (use apply(), not raiseEvent())
    this.apply(new OrderPlaced(command.items));
  }

  @EventSourcingHandler('OrderPlaced')
  private onPlaced(event: OrderPlaced): void {
    this.status = 'placed';
  }
}
```

**Key Changes:**
- Add `@Aggregate('AggregateType')` decorator to class
- Add `@CommandHandler('CommandName')` to command methods
- Change method signature to accept command object
- Add validation at start of method
- Call `this.initialize(aggregateId)` for creation commands
- Change `this.raiseEvent()` to `this.apply()`

---

### Step 3: Delete Separate Handler Files

**Before:**
```
place-order/
├── PlaceOrderCommand.ts
├── OrderPlacedEvent.ts
├── PlaceOrderHandler.ts  ← Delete this
├── OrderAggregate.ts
└── PlaceOrder.test.ts
```

**After:**
```
place-order/
├── PlaceOrderCommand.ts
├── OrderPlacedEvent.ts
├── OrderAggregate.ts      ← Now contains command handler
└── PlaceOrder.test.ts
```

---

### Step 4: Update Command Dispatching

**Before:**
```typescript
// PlaceOrderHandler.ts
export class PlaceOrderHandler {
  constructor(private repository: Repository<OrderAggregate>) {}

  async handle(command: PlaceOrderCommand): Promise<void> {
    const aggregate = new OrderAggregate();
    aggregate.placeOrder(command.items);  // ← Direct method call
    await this.repository.save(aggregate);
  }
}

// Usage
await handler.handle({ orderId: '123', customerId: 'c1', items: [...] });
```

**After:**
```typescript
// CommandBus.ts or inline
const aggregate = new OrderAggregate();
const command = new PlaceOrderCommand('order-123', 'customer-1', [...items]);
aggregate.handleCommand(command);  // ← Dispatches to @CommandHandler
await repository.save(aggregate);

// Or with repository:
let aggregate = await repository.load(command.aggregateId) || new OrderAggregate();
aggregate.handleCommand(command);
await repository.save(aggregate);
```

**Key Changes:**
- Remove handler class entirely
- Create command instance: `new Command(...)`
- Call `aggregate.handleCommand(command)` instead of direct method
- Use repository pattern for load/save

---

### Step 5: Update Tests

**Before:**
```typescript
// PlaceOrder.test.ts
describe('PlaceOrderHandler', () => {
  it('should place order', async () => {
    const repository = new InMemoryRepository();
    const handler = new PlaceOrderHandler(repository);
    
    await handler.handle({
      orderId: 'order-1',
      customerId: 'customer-1',
      items: [...]
    });
    
    const aggregate = await repository.load('order-1');
    expect(aggregate.getStatus()).toBe('placed');
  });
});
```

**After:**
```typescript
// PlaceOrder.test.ts
describe('OrderAggregate', () => {
  it('should place order', () => {
    // Arrange
    const aggregate = new OrderAggregate();
    const command = new PlaceOrderCommand(
      'order-1',
      'customer-1',
      [{ productId: 'p1', quantity: 1, price: 10 }]
    );

    // Act
    (aggregate as any).handleCommand(command);

    // Assert
    const events = aggregate.getUncommittedEvents();
    expect(events).toHaveLength(1);
    expect(events[0].eventType).toBe('OrderPlaced');
  });

  it('should reject empty orders', () => {
    const aggregate = new OrderAggregate();
    const command = new PlaceOrderCommand('order-1', 'customer-1', []);

    expect(() => (aggregate as any).handleCommand(command)).toThrow(
      'Order must have items'
    );
  });
});
```

**Key Changes:**
- Test aggregate directly (no handler needed)
- Create command instances
- Call `handleCommand()` to dispatch
- Assert on uncommitted events
- Test validation logic separately

---

## Migration Checklist

For each feature/command:

### Phase 1: Preparation
- [ ] Read existing handler logic
- [ ] Identify business rules and validation
- [ ] Note any side effects or dependencies

### Phase 2: Command Migration
- [ ] Convert command interface to class
- [ ] Add `aggregateId` as first constructor parameter
- [ ] Make all properties `readonly`
- [ ] Update imports in other files

### Phase 3: Aggregate Migration
- [ ] Add `@Aggregate('TypeName')` to aggregate class
- [ ] Add `@CommandHandler('CommandName')` to methods
- [ ] Move handler validation logic into command method
- [ ] Change method signature to accept command object
- [ ] Add `this.initialize(aggregateId)` for creation commands
- [ ] Change `this.raiseEvent()` to `this.apply()`
- [ ] Ensure event handlers only update state

### Phase 4: Handler Removal
- [ ] Delete separate handler class file
- [ ] Update command bus/dispatcher
- [ ] Remove handler imports
- [ ] Update dependency injection

### Phase 5: Testing
- [ ] Update test imports
- [ ] Rewrite tests to test aggregate directly
- [ ] Add validation tests
- [ ] Test command dispatching
- [ ] Run full test suite
- [ ] Manual testing

### Phase 6: Documentation
- [ ] Update README if exists
- [ ] Update architecture diagrams
- [ ] Update onboarding docs
- [ ] Add migration notes to CHANGELOG

---

## Common Patterns

### Pattern 1: Command with Multiple Steps

**Old:**
```typescript
class PlaceOrderHandler {
  async handle(command: PlaceOrderCommand) {
    const aggregate = new OrderAggregate();
    const total = this.calculateTotal(command.items);
    const validated = this.validateStock(command.items);
    aggregate.place(command.customerId, validated, total);
    await this.repository.save(aggregate);
  }
}
```

**New:**
```typescript
@Aggregate('Order')
class OrderAggregate extends AggregateRoot<OrderEvent> {
  @CommandHandler('PlaceOrderCommand')
  placeOrder(command: PlaceOrderCommand): void {
    // All logic moves into aggregate
    const total = this.calculateTotal(command.items);
    const validated = this.validateStock(command.items);
    
    this.initialize(command.aggregateId);
    this.apply(new OrderPlaced(command.customerId, validated, total));
  }

  private calculateTotal(items: Item[]): number {
    return items.reduce((sum, item) => sum + item.price * item.quantity, 0);
  }

  private validateStock(items: Item[]): Item[] {
    // validation logic
    return items;
  }
}
```

### Pattern 2: Loading Existing Aggregate

**Old:**
```typescript
class UpdateOrderHandler {
  async handle(command: UpdateOrderCommand) {
    const aggregate = await this.repository.load(command.orderId);
    if (!aggregate) throw new Error('Not found');
    aggregate.update(command.status);
    await this.repository.save(aggregate);
  }
}
```

**New:**
```typescript
// In command bus or application service
async function handleCommand(command: UpdateOrderCommand) {
  const aggregate = await repository.load(command.aggregateId);
  if (!aggregate) throw new Error('Order not found');
  
  aggregate.handleCommand(command);
  await repository.save(aggregate);
}

// In aggregate
@CommandHandler('UpdateOrderCommand')
updateOrder(command: UpdateOrderCommand): void {
  if (!this.id) throw new Error('Order not initialized');
  // validation...
  this.apply(new OrderUpdated(command.status));
}
```

### Pattern 3: Integration Events

**Old:**
```typescript
class PlaceOrderHandler {
  async handle(command: PlaceOrderCommand) {
    const aggregate = new OrderAggregate();
    aggregate.place(command);
    await this.repository.save(aggregate);
    
    // Publish integration event
    await this.eventBus.publish(new OrderPlacedIntegrationEvent(
      aggregate.id,
      aggregate.getCustomerId()
    ));
  }
}
```

**New:**
```typescript
@Aggregate('Order')
class OrderAggregate extends AggregateRoot<OrderEvent> {
  @CommandHandler('PlaceOrderCommand')
  placeOrder(command: PlaceOrderCommand): void {
    this.initialize(command.aggregateId);
    
    // Domain event
    this.apply(new OrderPlaced(command.customerId, command.items));
    
    // Integration event (for other contexts)
    this.apply(new OrderPlacedIntegrationEvent(
      command.aggregateId,
      command.customerId
    ));
  }
}
```

---

## Troubleshooting

### Issue: "No @CommandHandler found for command type"

**Cause:** Command class name doesn't match decorator string.

**Solution:**
```typescript
// Command
export class PlaceOrderCommand { ... }  // ← Name must match

// Aggregate
@CommandHandler('PlaceOrderCommand')  // ← Exact match required
placeOrder(command: PlaceOrderCommand) { ... }
```

### Issue: "Cannot raise events on aggregate without ID"

**Cause:** Forgot to initialize aggregate before applying events.

**Solution:**
```typescript
@CommandHandler('PlaceOrderCommand')
placeOrder(command: PlaceOrderCommand): void {
  this.initialize(command.aggregateId);  // ← Add this!
  this.apply(new OrderPlaced(...));
}
```

### Issue: Tests fail with "Property 'handleCommand' does not exist"

**Cause:** Need to cast aggregate to `any` to access protected method.

**Solution:**
```typescript
(aggregate as any).handleCommand(command);  // ← Cast to any
```

### Issue: "aggregateId is required"

**Cause:** Command class missing `aggregateId` property.

**Solution:**
```typescript
class MyCommand {
  constructor(
    public readonly aggregateId: string,  // ← Must be first!
    public readonly otherData: string
  ) {}
}
```

---

## FAQ

**Q: Can I keep some handlers as separate classes?**  
A: Yes, but it's not recommended. For consistency, migrate all commands to use `@CommandHandler`.

**Q: What about query handlers?**  
A: Query handlers can remain as separate classes. This pattern is specifically for commands.

**Q: How do I handle cross-aggregate operations?**  
A: Use sagas/process managers or integration events. Aggregates should remain independent.

**Q: What about async operations in handlers?**  
A: Command handlers should be synchronous. Move async operations to sagas or application services.

**Q: Can I have multiple `@CommandHandler` methods per aggregate?**  
A: Yes! Each aggregate can handle multiple commands:
```typescript
@Aggregate('Order')
class OrderAggregate extends AggregateRoot<OrderEvent> {
  @CommandHandler('PlaceOrderCommand')
  placeOrder(command: PlaceOrderCommand) { ... }

  @CommandHandler('CancelOrderCommand')
  cancelOrder(command: CancelOrderCommand) { ... }

  @CommandHandler('ShipOrderCommand')
  shipOrder(command: ShipOrderCommand) { ... }
}
```

**Q: What about repository injection?**  
A: Repositories are used outside the aggregate, in command buses or application services.

**Q: How do I test validation?**  
A: Create test cases that pass invalid commands and assert exceptions are thrown.

---

## Resources

- **[ADR-004](./docs/adrs/ADR-004-command-handlers-in-aggregates.md)** - Architecture Decision Record
- **[VSA + Event Sourcing Guide](./docs-site/docs/guides/vsa-event-sourcing-guide.md)** - Complete guide
- **[Examples](./examples/)** - All examples use the new pattern
- **[CLAUDE.md](./CLAUDE.md)** - Agent guidance

---

## Support

**Need help migrating?**
- Check examples: `examples/001-basic-store-ts` through `010-observability-ts`
- Review VSA examples: `vsa/examples/01-todo-list-ts`, `02-library-management-ts`
- Read the comprehensive guide: `docs-site/docs/guides/vsa-event-sourcing-guide.md`
- Check ADR-004 for the reasoning behind this pattern

**Found an issue?**
- Open a GitHub issue
- Include code samples
- Describe expected vs actual behavior

---

**Happy migrating!** 🚀

The new pattern will make your code simpler, clearer, and more maintainable.

