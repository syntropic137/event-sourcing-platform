# ADR-004: Command Handlers in Aggregates

**Status:** ✅ Accepted & Implemented  
**Date:** 2025-11-05  
**Decision Makers:** Architecture Team  
**Related:** Event Sourcing, CQRS, DDD Patterns

**Implementation Status:**
- ✅ All 6 examples refactored to compliance
- ✅ 17 aggregates created across examples
- ✅ 40+ command handlers implemented  
- ✅ Pattern validated in production-ready examples

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

### Architectural Context: Hexagonal Architecture

This decision is foundational to our **Hexagonal Event-Sourced Architecture** pattern. Aggregates with integrated command handlers form the **core domain layer** (center of the hexagon), completely isolated from infrastructure concerns.

**Domain Layer Structure:**
```
domain/                    ← HEXAGON CORE (pure business logic)
├── TaskAggregate.ts       ← Aggregates with @CommandHandler methods
├── CartAggregate.ts       ← Shared across ALL features/slices
├── commands/              ← Command definitions
│   ├── CreateTaskCommand.ts
│   └── CompleteTaskCommand.ts
└── events/                ← Domain events
    ├── TaskCreatedEvent.ts
    └── TaskCompletedEvent.ts
```

**Key Principles:**
- ✅ Aggregates live in `domain/` folder (NOT inside feature slices)
- ✅ Aggregates are **shared** across all vertical slices/features
- ✅ Business logic is encapsulated in aggregate methods
- ✅ External code (adapters, slices) cannot bypass validation
- ✅ Dependencies point INWARD: Adapters → Infrastructure → Domain

**Relationship to Vertical Slices:**
```
slices/create-task/        ← ADAPTER (Hexagon outside)
  └── CreateTaskController.ts  (HTTP → CommandBus → Aggregate)

infrastructure/            ← APPLICATION SERVICES
  └── CommandBus.ts       (Routes commands to aggregates)

domain/                    ← CORE (Hexagon center)
  └── TaskAggregate.ts    (@CommandHandler methods)
```

Vertical slices are **thin adapters** that translate external protocols (HTTP, CLI, gRPC) into commands, which are then routed to aggregates via the CommandBus. The aggregate's `@CommandHandler` methods contain ALL business logic.

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

## Language-Specific Implementations

ADR-004 is implemented across all three language SDKs (TypeScript, Python, Rust). While the pattern is consistent, each language uses its native features.

### TypeScript Pattern (Decorators)

TypeScript uses decorators to mark command handlers and event sourcing handlers:

```typescript
import { Aggregate, AggregateRoot, CommandHandler, EventSourcingHandler } from '@event-sourcing-platform/typescript';

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

  getAggregateType(): string {
    return 'Task';
  }
}
```

**Key Features:**
- `@Aggregate` decorator on class
- `@CommandHandler` decorator on command methods
- `@EventSourcingHandler` decorator on event handlers
- `apply()` method to emit events
- Commands as classes with `aggregateId` property

### Python Pattern (Decorators)

Python also uses decorators, following the same pattern:

```python
from event_sourcing.decorators import aggregate, command_handler, event_sourcing_handler
from event_sourcing.core.aggregate import AggregateRoot

@aggregate('Task')
class TaskAggregate(AggregateRoot):
    """Task Aggregate - ADR-004 Compliant"""
    
    def __init__(self):
        super().__init__()
        self.task_id: Optional[str] = None
        self.title: Optional[str] = None
        self.completed: bool = False
    
    # COMMAND HANDLER - Business logic and validation
    @command_handler('CreateTaskCommand')
    def create_task(self, command: CreateTaskCommand) -> None:
        # 1. Validate business rules
        if not command.title or command.title.strip() == '':
            raise ValueError('Task title is required')
        if self.task_id is not None:
            raise ValueError('Task already exists')
        
        # 2. Initialize aggregate
        self._initialize(command.id)
        
        # 3. Apply event
        event = TaskCreatedEvent(
            event_type='TaskCreated',
            id=command.id,
            title=command.title
        )
        self._apply(event)
    
    # EVENT SOURCING HANDLER - State updates only
    @event_sourcing_handler('TaskCreated')
    def _on_task_created(self, event: TaskCreatedEvent) -> None:
        # NO validation - just state updates
        self.task_id = event.id
        self.title = event.title
```

**Key Features:**
- `@aggregate('Task')` decorator on class
- `@command_handler` decorator on command methods
- `@event_sourcing_handler` decorator on event handlers
- `_apply()` method to emit events
- Commands as Pydantic models or classes

### Rust Pattern (Traits)

Rust uses traits instead of decorators, but follows the same pattern:

```rust
use async_trait::async_trait;
use event_sourcing_rust::prelude::*;

/// Task aggregate
#[derive(Debug, Clone, Default)]
struct Task {
    id: Option<String>,
    title: String,
    completed: bool,
    version: u64,
}

/// Task events
#[derive(Debug, Clone, Serialize, Deserialize)]
enum TaskEvent {
    Created { id: String, title: String },
    Completed,
}

impl DomainEvent for TaskEvent {
    fn event_type(&self) -> &'static str {
        match self {
            TaskEvent::Created { .. } => "TaskCreated",
            TaskEvent::Completed => "TaskCompleted",
        }
    }
}

// Aggregate trait for state management
impl Aggregate for Task {
    type Event = TaskEvent;
    type Error = Error;

    fn aggregate_id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    fn version(&self) -> u64 {
        self.version
    }

    // EVENT SOURCING - State updates only
    fn apply_event(&mut self, event: &Self::Event) -> Result<()> {
        match event {
            TaskEvent::Created { id, title } => {
                self.id = Some(id.clone());
                self.title = title.clone();
                self.version += 1;
            }
            TaskEvent::Completed => {
                self.completed = true;
                self.version += 1;
            }
        }
        Ok(())
    }
}

// Task commands
#[derive(Debug, Clone)]
enum TaskCommand {
    CreateTask { id: String, title: String },
    CompleteTask,
}

impl Command for TaskCommand {}

// AggregateRoot trait for command handling
#[async_trait]
impl AggregateRoot for Task {
    type Command = TaskCommand;

    // COMMAND HANDLER - Business logic and validation
    async fn handle_command(&self, command: Self::Command) 
        -> Result<Vec<Self::Event>> 
    {
        match command {
            TaskCommand::CreateTask { id, title } => {
                // 1. Validate business rules
                if title.is_empty() {
                    return Err(Error::invalid_command("Title is required"));
                }
                if self.id.is_some() {
                    return Err(Error::invalid_command("Task already exists"));
                }
                
                // 2. Return events to apply
                Ok(vec![TaskEvent::Created { id, title }])
            }
            
            TaskCommand::CompleteTask => {
                if self.id.is_none() {
                    return Err(Error::invalid_command("Task does not exist"));
                }
                if self.completed {
                    return Err(Error::invalid_command("Task already completed"));
                }
                Ok(vec![TaskEvent::Completed])
            }
        }
    }
}
```

**Key Features:**
- `Aggregate` trait for state management (`apply_event`)
- `AggregateRoot` trait for command handling (`handle_command`)
- `#[async_trait]` for async methods
- Commands as enums
- Returns `Result<Vec<Event>>` from command handlers

### Pattern Comparison

| Aspect | TypeScript | Python | Rust |
|--------|-----------|---------|------|
| **Aggregate Marking** | `@Aggregate` decorator | `@aggregate` decorator | `impl Aggregate` trait |
| **Command Handlers** | `@CommandHandler` decorator | `@command_handler` decorator | `impl AggregateRoot` trait |
| **Event Handlers** | `@EventSourcingHandler` decorator | `@event_sourcing_handler` decorator | `apply_event()` method |
| **Event Emission** | `this.apply(event)` | `self._apply(event)` | Return `Vec<Event>` |
| **Commands** | Classes with `aggregateId` | Classes/Pydantic models | Enums or structs |
| **Async Support** | Native async/await | Native async/await | `#[async_trait]` macro |
| **Type Safety** | TypeScript types | Python type hints | Rust strong typing |

### Universal Principles

Regardless of language, all implementations follow these ADR-004 principles:

1. **Command handlers integrated in aggregates** (not separate classes)
2. **Business validation in command handlers**
3. **State updates only in event sourcing handlers**
4. **Events represent immutable facts**
5. **Commands validated before events are applied**
6. **Aggregates enforce business invariants**

## Migration Path

1. ✅ Add `handleCommand()` to `AggregateRoot`
2. ✅ Update examples to use @CommandHandler
3. ✅ Delete separate handler classes
4. ✅ Update documentation
5. ✅ Multi-language implementation complete
6. 🔄 Create migration guide for users
7. 🔄 Add deprecation warnings to old pattern

## Review and Approval

- **Proposed:** 2025-11-05
- **Reviewed:** Architecture Team
- **Approved:** 2025-11-05
- **Status:** Implemented

## Related ADRs

- ADR-002: Convention Over Configuration
- ADR-003: Language-Native Build Tools
- ADR-005: Hexagonal Architecture for Event-Sourced Systems (architectural context)
- ADR-006: Domain Organization Pattern (where aggregates live)
- ADR-008: Vertical Slices as Hexagonal Adapters (how slices use aggregates)
- ADR-009: CQRS Pattern Implementation (command vs query separation)

---

## Examples Demonstrating This Pattern

All examples in this repository now follow ADR-004 across three languages:

### TypeScript Examples (/examples)
- **007-inventory-complete-ts** - ProductAggregate with 4 command handlers
- **007-ecommerce-complete-ts** - Product/Order/Customer aggregates (11 commands)
- **008-banking-complete-ts** - Account/Transfer/Customer aggregates (9 commands)
- **008-observability-ts** - User/Order/Payment/System aggregates (4 commands)
- **009-web-dashboard-ts** - Product/Order aggregates with Express API (5 commands)

### TypeScript VSA Examples (/vsa/examples)
- **01-todo-list-ts** - TaskAggregate with 3 command handlers
- **02-library-management-ts** - Book/Loan/Notification aggregates (7 commands)

### Python Examples (/vsa/examples)
- **05-todo-list-py** - TaskAggregate with @command_handler decorators (3 commands)

### Rust Examples (/event-sourcing/rust/examples)
- **order_processing.rs** - OrderAggregate with AggregateRoot trait (7 commands)
- **basic_aggregate.rs** - UserAggregate with AggregateRoot trait (5 commands)

**Total: 10 examples across 3 languages, all ADR-004 compliant**

All examples demonstrate:
- ✅ Command handlers integrated in aggregates
- ✅ Business validation in command handlers
- ✅ State-only updates in event sourcing handlers
- ✅ Commands as classes/enums with aggregate identifier
- ✅ Events applied via `apply()` / `_apply()` / returned from `handle_command()`
- ✅ Language-appropriate patterns (decorators for TS/Python, traits for Rust)

---

**Last Updated:** 2025-11-06  
**Supersedes:** None  
**Superseded By:** None
