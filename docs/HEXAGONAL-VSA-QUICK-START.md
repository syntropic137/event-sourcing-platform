# Hexagonal Event-Sourced VSA - Quick Start Guide

**Version:** 1.0.0  
**Last Updated:** 2025-11-06

This guide provides a quick overview of the Hexagonal Event-Sourced Vertical Slice Architecture pattern and how to use it in your projects.

---

## 📖 What is Hexagonal Event-Sourced VSA?

A powerful architectural pattern that combines:

1. **Hexagonal Architecture** - Isolates business logic from infrastructure
2. **Vertical Slice Architecture** - Organizes code by feature, not layer
3. **Event Sourcing** - Captures all state changes as immutable events
4. **CQRS** - Separates read and write operations

---

## 🏗️ Project Structure

```
my-project/
├── domain/                     ← CORE: Pure business logic
│   ├── TaskAggregate.ts        ← Aggregates with @CommandHandler
│   ├── CartAggregate.ts
│   │
│   ├── commands/               ← Command definitions
│   │   ├── tasks/
│   │   │   ├── CreateTaskCommand.ts
│   │   │   └── CompleteTaskCommand.ts
│   │   └── cart/
│   │       └── AddItemCommand.ts
│   │
│   ├── queries/                ← Query definitions
│   │   ├── GetTaskByIdQuery.ts
│   │   └── GetCartSummaryQuery.ts
│   │
│   └── events/                 ← Event definitions
│       ├── TaskCreatedEvent.ts      ← Current version
│       ├── TaskCompletedEvent.ts
│       │
│       ├── _versioned/         ← Old event versions
│       │   └── TaskCreatedEvent_v1.ts
│       │
│       └── _upcasters/         ← Event migration logic
│           └── TaskCreatedEvent_Upcaster_v1_v2.ts
│
├── infrastructure/             ← APPLICATION SERVICES: Routing & coordination
│   ├── CommandBus.ts
│   ├── QueryBus.ts
│   ├── EventBus.ts
│   └── TaskRepository.ts
│
└── slices/                     ← ADAPTERS: Thin protocol translators
    ├── create-task/            ← Command slice
    │   ├── CreateTaskController.ts
    │   ├── CreateTaskController.test.ts
    │   └── slice.yaml
    │
    ├── get-task/               ← Query slice
    │   ├── GetTaskController.ts
    │   ├── TaskProjection.ts
    │   ├── TaskProjection.test.ts
    │   └── slice.yaml
    │
    └── task-notification-saga/ ← Saga slice
        ├── TaskNotificationSaga.ts
        ├── TaskNotificationSaga.test.ts
        └── slice.yaml
```

---

## 🎯 Key Principles

### 1. Domain is Isolated

**✅ DO:**
- Pure business logic in aggregates
- No infrastructure imports
- No external library dependencies (except utils)

**❌ DON'T:**
- Import from infrastructure
- Import from slices
- Add HTTP/database code

### 2. Slices are Thin

**✅ DO:**
- Translate requests to commands/queries
- Dispatch via CommandBus/QueryBus
- Keep under 50 lines

**❌ DON'T:**
- Add business logic
- Validate business rules
- Import from other slices

### 3. Dependencies Point Inward

```
Adapters (slices/) ──▶ Infrastructure ──▶ Domain
                         ⬆                  ⬆
                         │                  │
                    Read Only          Pure Logic
```

---

## 🚀 Getting Started

### Step 1: Create vsa.yaml

```yaml
version: 2
architecture: "hexagonal-event-sourced-vsa"
language: "typescript"

domain:
  path: "domain"
  aggregates:
    pattern: "*Aggregate.ts"
  events:
    path: "events"
    versioning:
      enabled: true
      format: "simple"  # 'v1', 'v2', 'v3'
      require_upcasters: true

slices:
  path: "slices"
  command:
    must_use: "CommandBus"
    max_lines: 50
    no_business_logic: true

validation:
  architecture:
    enforce_hexagonal: true
    slices_isolated: true
  event_sourcing:
    require_event_versioning: true
```

### Step 2: Create an Aggregate

```typescript
// domain/TaskAggregate.ts
import { Aggregate, CommandHandler, EventSourcingHandler } from '@vsa/core';
import { CreateTaskCommand } from './commands/tasks/CreateTaskCommand';
import { TaskCreatedEvent } from './events/TaskCreatedEvent';

@Aggregate()
export class TaskAggregate {
  private id: string;
  private title: string;
  private completed: boolean = false;

  @CommandHandler
  handle(command: CreateTaskCommand): void {
    // Business validation
    if (!command.title || command.title.length === 0) {
      throw new Error('Task title is required');
    }

    // Emit event
    this.apply(new TaskCreatedEvent(
      command.aggregateId,
      command.title,
      command.description
    ));
  }

  @EventSourcingHandler
  on(event: TaskCreatedEvent): void {
    // Update internal state
    this.id = event.aggregateId;
    this.title = event.title;
  }
}
```

### Step 3: Create a Command

```typescript
// domain/commands/tasks/CreateTaskCommand.ts
export class CreateTaskCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly title: string,
    public readonly description?: string
  ) {}
}
```

### Step 4: Create an Event (Versioned)

```typescript
// domain/events/TaskCreatedEvent.ts
import { Event } from '@vsa/core';

@Event('TaskCreated', 'v1')
export class TaskCreatedEvent {
  constructor(
    public readonly aggregateId: string,
    public readonly title: string,
    public readonly description?: string
  ) {}
}
```

### Step 5: Create a Command Slice

```typescript
// slices/create-task/CreateTaskController.ts
import { RestController, Post, Body } from '@vsa/adapters';
import { CommandBus } from '../../infrastructure/CommandBus';
import { CreateTaskCommand } from '../../domain/commands/tasks/CreateTaskCommand';

@RestController('/api/tasks')
export class CreateTaskController {
  constructor(private commandBus: CommandBus) {}

  @Post('/')
  async handle(@Body() request: CreateTaskRequest): Promise<void> {
    // Thin adapter: Just translate and dispatch
    const command = new CreateTaskCommand(
      this.generateId(),
      request.title,
      request.description
    );

    await this.commandBus.send(command);
  }

  private generateId(): string {
    return crypto.randomUUID();
  }
}

interface CreateTaskRequest {
  title: string;
  description?: string;
}
```

### Step 6: Create slice.yaml Metadata

```yaml
# slices/create-task/slice.yaml
name: "create-task"
type: "command"
description: "Creates a new task"

command:
  command_type: "CreateTaskCommand"
  aggregate: "TaskAggregate"

adapters:
  rest:
    enabled: true
    routes:
      - method: "POST"
        path: "/api/tasks"
        handler: "CreateTaskController.handle"

testing:
  test_files:
    - "CreateTaskController.test.ts"
```

### Step 7: Validate Architecture

```bash
vsa validate --config vsa.yaml
```

---

## 📝 Event Versioning Example

### When to Version

When you need to change an event's schema:

1. Add a field
2. Remove a field
3. Rename a field
4. Change field type

### How to Version

#### Step 1: Move Old Version

```bash
# Move current event to _versioned/
mv domain/events/TaskCreatedEvent.ts \
   domain/events/_versioned/TaskCreatedEvent_v1.ts
```

#### Step 2: Mark as Deprecated

```typescript
// domain/events/_versioned/TaskCreatedEvent_v1.ts
@Event('TaskCreated', 'v1')
@Deprecated('v2')
export class TaskCreatedEvent_v1 {
  constructor(
    public readonly aggregateId: string,
    public readonly title: string
  ) {}
}
```

#### Step 3: Create New Version

```typescript
// domain/events/TaskCreatedEvent.ts
@Event('TaskCreated', 'v2')
export class TaskCreatedEvent {
  constructor(
    public readonly aggregateId: string,
    public readonly title: string,
    public readonly assignee: string  // NEW FIELD
  ) {}
}
```

#### Step 4: Create Upcaster

```typescript
// domain/events/_upcasters/TaskCreatedEvent_Upcaster_v1_v2.ts
import { Upcaster } from '@vsa/core';
import { TaskCreatedEvent_v1 } from '../_versioned/TaskCreatedEvent_v1';
import { TaskCreatedEvent } from '../TaskCreatedEvent';

@Upcaster('TaskCreated', { from: 'v1', to: 'v2' })
export class TaskCreatedEventUpcaster {
  upcast(event: TaskCreatedEvent_v1): TaskCreatedEvent {
    return new TaskCreatedEvent(
      event.aggregateId,
      event.title,
      'unassigned'  // Default value for new field
    );
  }
}
```

---

## 🧪 Testing

### Test Aggregate in Isolation

```typescript
// domain/TaskAggregate.test.ts
import { TaskAggregate } from './TaskAggregate';
import { CreateTaskCommand } from './commands/tasks/CreateTaskCommand';
import { TaskCreatedEvent } from './events/TaskCreatedEvent';

describe('TaskAggregate', () => {
  it('should create task', () => {
    const aggregate = new TaskAggregate();
    const command = new CreateTaskCommand('123', 'My Task');

    const events = aggregate.handle(command);

    expect(events).toHaveLength(1);
    expect(events[0]).toBeInstanceOf(TaskCreatedEvent);
    expect(events[0].title).toBe('My Task');
  });

  it('should reject empty title', () => {
    const aggregate = new TaskAggregate();
    const command = new CreateTaskCommand('123', '');

    expect(() => aggregate.handle(command))
      .toThrow('Task title is required');
  });
});
```

### Test Slice in Isolation

```typescript
// slices/create-task/CreateTaskController.test.ts
import { CreateTaskController } from './CreateTaskController';

describe('CreateTaskController', () => {
  it('should dispatch command', async () => {
    const mockCommandBus = { send: jest.fn() };
    const controller = new CreateTaskController(mockCommandBus);

    await controller.handle({ title: 'My Task' });

    expect(mockCommandBus.send).toHaveBeenCalledWith(
      expect.objectContaining({ title: 'My Task' })
    );
  });
});
```

---

## 📚 Further Reading

- **[ADR Index](./adrs/ADR-INDEX.md)** - Complete architectural overview
- **[ADR-005](./adrs/ADR-005-hexagonal-architecture-event-sourcing.md)** - Hexagonal Architecture
- **[ADR-006](./adrs/ADR-006-domain-organization-pattern.md)** - Domain Organization
- **[ADR-007](./adrs/ADR-007-event-versioning-upcasters.md)** - Event Versioning
- **[ADR-008](./adrs/ADR-008-vertical-slices-hexagonal-adapters.md)** - Vertical Slices
- **[vsa.reference.yaml](../vsa/examples/vsa.reference.yaml)** - Complete config reference
- **[slice.reference.yaml](../vsa/examples/slice.reference.yaml)** - Slice metadata reference

---

## 🆘 Common Issues

### Issue: "Domain imports from infrastructure"

**Error:** `HEX001: Aggregate 'TaskAggregate' imports from infrastructure`

**Fix:** Remove infrastructure imports from domain. Use dependency injection via aggregate constructor instead.

### Issue: "Slice contains business logic"

**Error:** `HEX003: Slice 'create-task' contains business logic`

**Fix:** Move validation/calculations to aggregate. Slice should only translate and dispatch.

### Issue: "Event missing version"

**Error:** `EVT002: Event 'TaskCreatedEvent' missing version parameter`

**Fix:** Add version to @Event decorator: `@Event('TaskCreated', 'v1')`

### Issue: "Missing upcaster"

**Error:** `EVT003: Event 'TaskCreatedEvent' changed but no upcaster found`

**Fix:** Create upcaster in `domain/events/_upcasters/` folder.

---

## ✅ Checklist

When creating a new feature:

- [ ] Define command in `domain/commands/{feature}/`
- [ ] Add command handler to aggregate
- [ ] Define event in `domain/events/` with `@Event` decorator
- [ ] Add event sourcing handler to aggregate
- [ ] Create slice in `slices/{feature}/`
- [ ] Add controller with thin adapter logic
- [ ] Create `slice.yaml` metadata
- [ ] Write tests (aggregate + slice)
- [ ] Run `vsa validate`
- [ ] All validation rules pass

---

## 🎉 You're Ready!

You now have everything you need to build applications with the Hexagonal Event-Sourced VSA pattern. Start small, follow the principles, and let the architecture guide you to clean, maintainable code.

**Happy coding!** 🚀

