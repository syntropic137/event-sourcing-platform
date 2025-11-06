# ADR-004: Command Handlers in Aggregates

**Status:** Accepted  
**Date:** 2025-11-05  
**Decision Makers:** Architecture Team  
**Related:** Event Sourcing, CQRS, DDD Patterns

## Context

In our event sourcing implementation, we initially had command handlers as separate classes that operated on aggregates. This pattern, while common in some frameworks, led to several issues:

1. **Scattered Business Logic:** Command validation and business rules were separated from the aggregate state
2. **Boilerplate Code:** Each command required a separate handler class with repository management
3. **Inconsistent Pattern:** Different from industry-standard event sourcing frameworks (Axon, EventStore, etc.)
4. **Teaching Wrong Patterns:** Examples taught incorrect event sourcing principles

### The Incorrect Pattern (Before)

```typescript
// Separate handler class
export class CreateTaskHandler {
  constructor(private eventStore: EventStoreClient) {}

  async handle(command: CreateTaskCommand): Promise<void> {
    // Load/create aggregate
    const aggregate = new TaskAggregate();
    
    // Validate
    if (!command.title) {
      throw new Error('Title required');
    }
    
    // Create event
    aggregate.createTask(command.id, command.title, command.description);
    
    // Save
    await this.repository.save(aggregate);
  }
}

// Aggregate without command handlers
export class TaskAggregate extends AggregateRoot<TaskEvent> {
  createTask(id: string, title: string, description?: string): void {
    this.apply(new TaskCreatedEvent(id, title, description));
  }
  
  @EventSourcingHandler('TaskCreated')
  private onTaskCreated(event: TaskCreatedEvent): void {
    this.initialize(event.id);
    this.title = event.title;
  }
}
```

### Problems with This Approach

1. **Violation of DDD Principles:** Aggregates should encapsulate business logic and rules
2. **Two Places for Validation:** Command handler AND aggregate both might validate
3. **Redundant Code:** Repository loading/saving logic repeated in every handler
4. **State Leakage:** Aggregate state might be accessed from outside the aggregate
5. **Testing Complexity:** Need to test both handler and aggregate separately

## Decision

We implement **command dispatching directly in aggregates** using the `@CommandHandler` decorator, following the pattern from "Understanding Event Sourcing" and industry frameworks.

### The Correct Pattern (After)

```typescript
// Command as a class
export class CreateTaskCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly title: string,
    public readonly description?: string
  ) {}
}

// Aggregate with command AND event handlers
@Aggregate('Task')
export class TaskAggregate extends AggregateRoot<TaskEvent> {
  private title: string | null = null;
  private completed: boolean = false;

  // COMMAND HANDLER - Validates and creates events
  @CommandHandler('CreateTaskCommand')
  createTask(command: CreateTaskCommand): void {
    // 1. Validate business rules
    if (!command.title || command.title.trim() === '') {
      throw new Error('Task title is required');
    }
    if (this.id !== null) {
      throw new Error('Task already exists');
    }

    // 2. Initialize aggregate
    this.initialize(command.aggregateId);

    // 3. Apply event
    this.apply(new TaskCreatedEvent(
      command.aggregateId,
      command.title,
      command.description
    ));
  }

  // EVENT SOURCING HANDLER - Updates state only
  @EventSourcingHandler('TaskCreated')
  private onTaskCreated(event: TaskCreatedEvent): void {
    // NO validation, NO business logic
    // ONLY state updates
    this.title = event.title;
    this.description = event.description;
  }
}

// Simplified infrastructure
export class CommandBus {
  async send(command: Command): Promise<void> {
    // Load aggregate
    let aggregate = await this.repository.load(command.aggregateId);
    if (!aggregate) {
      aggregate = new TaskAggregate();
    }

    // Dispatch to @CommandHandler
    aggregate.handleCommand(command);

    // Save
    await this.repository.save(aggregate);
  }
}
```

## Rationale

### 1. **Follows Event Sourcing Principles**

The pattern clearly separates two concerns:
- **Command Handlers:** Validate + Emit Events
- **Event Handlers:** Update State

This is the foundation of proper event sourcing as described in literature.

### 2. **Consistent with Industry Standards**

- **Axon Framework (Java):** Uses `@CommandHandler` on aggregate methods
- **EventStore:** Recommends commands on aggregates
- **"Understanding Event Sourcing" book:** Explicitly teaches this pattern
- **NServiceBus:** Command handlers are aggregate methods

### 3. **Better Encapsulation**

- Business rules stay with the aggregate state
- Aggregate is the **single source of truth** for what operations are valid
- External code cannot bypass validation

### 4. **Less Boilerplate**

Before:
- Command class
- Handler class
- Aggregate method
- Event handler

After:
- Command class
- Aggregate command handler
- Aggregate event handler

Eliminates ~30% of code per command.

### 5. **Clearer Intent**

```typescript
@CommandHandler('CreateTask')  // <- This handles the command
createTask(command: CreateTaskCommand): void {
  // Validation + Event emission
}

@EventSourcingHandler('TaskCreated')  // <- This updates state
onTaskCreated(event: TaskCreatedEvent): void {
  // State updates only
}
```

The decorators make the intent explicit.

### 6. **Easier Testing**

```typescript
// Before: Test handler AND aggregate
describe('CreateTaskHandler', () => { ... });
describe('TaskAggregate', () => { ... });

// After: Test aggregate only
describe('TaskAggregate', () => {
  it('should create task with valid command', () => {
    const aggregate = new TaskAggregate();
    const command = new CreateTaskCommand('123', 'Test');
    
    aggregate.handleCommand(command);
    
    expect(aggregate.getUncommittedEvents()).toHaveLength(1);
  });
});
```

## Implementation

### Infrastructure Changes

1. **Export Command Handler Map Symbol** (`command.ts`)
   ```typescript
   export const COMMAND_HANDLER_MAP: unique symbol = Symbol('commandHandlerMap');
   ```

2. **Add `handleCommand()` Method** (`aggregate.ts`)
   ```typescript
   protected handleCommand<TCommand extends object>(command: TCommand): void {
     const commandType = (command as { constructor: { name: string } }).constructor.name;
     const handlers = (this.constructor as any)[COMMAND_HANDLER_MAP];
     
     if (!handlers?.has(commandType)) {
       throw new Error(`No @CommandHandler found for: ${commandType}`);
     }
     
     const methodName = handlers.get(commandType);
     (this as any)[methodName](command);
   }
   ```

3. **Update Command Bus** (application code)
   ```typescript
   async send(command: Command): Promise<void> {
     let aggregate = await this.repository.load(command.aggregateId) 
                     || new TaskAggregate();
     aggregate.handleCommand(command);
     await this.repository.save(aggregate);
   }
   ```

### Pattern Guidelines

**Command Handlers (`@CommandHandler`):**
- ✅ Validate business rules
- ✅ Check current state
- ✅ Throw errors for invalid operations
- ✅ Initialize aggregate (for creation commands)
- ✅ Apply events
- ❌ NO direct state modification
- ❌ NO external side effects

**Event Handlers (`@EventSourcingHandler`):**
- ✅ Update internal state
- ✅ Idempotent operations
- ❌ NO validation
- ❌ NO business logic
- ❌ NO side effects
- ❌ NO throwing errors

## Consequences

### Positive

1. **Correct Pattern:** Teaches proper event sourcing
2. **Less Code:** Eliminates separate handler classes
3. **Better Encapsulation:** Business rules with state
4. **Easier Testing:** Single aggregate to test
5. **Industry Standard:** Matches established frameworks

### Negative

1. **Migration Required:** Existing code needs refactoring
2. **Learning Curve:** Developers must understand decorator usage
3. **Aggregates Get Larger:** More methods per aggregate (but more cohesive)

### Neutral

1. **Command Routing:** Still need CommandBus, but it's simpler
2. **Repository Pattern:** Unchanged
3. **Event Store:** Unchanged

## Alternatives Considered

### Alternative 1: Keep Separate Handlers

**Rejected because:**
- Not industry standard
- Violates DDD principles
- More boilerplate
- Teaches wrong patterns

### Alternative 2: Hybrid Approach

Use separate handlers for complex validation, decorators for simple commands.

**Rejected because:**
- Inconsistent patterns confuse developers
- Hard to decide when to use which approach
- Still teaches two patterns instead of one correct pattern

### Alternative 3: No Decorators, Direct Method Calls

Just call aggregate methods directly from application layer.

**Rejected because:**
- Loses command/event distinction
- No automatic dispatching
- Harder to trace command flow
- Missing metadata for tooling

## References

- "Understanding Event Sourcing" - Alexey Zimarev
- Axon Framework Documentation
- EventStore Documentation
- "Domain-Driven Design" - Eric Evans
- "Implementing Domain-Driven Design" - Vaughn Vernon

## Migration Path

1. ✅ Add `handleCommand()` to `AggregateRoot`
2. ✅ Update examples to use @CommandHandler
3. ✅ Delete separate handler classes
4. ✅ Update documentation
5. 🔄 Create migration guide for users
6. 🔄 Add deprecation warnings to old pattern

## Review and Approval

- **Proposed:** 2025-11-05
- **Reviewed:** Architecture Team
- **Approved:** 2025-11-05
- **Status:** Implemented

## Related ADRs

- ADR-002: Convention Over Configuration
- ADR-003: Language-Native Build Tools

---

**Last Updated:** 2025-11-05  
**Supersedes:** None  
**Superseded By:** None
