---
sidebar_position: 10
---

# Migration Guide: AutoDispatchAggregate to AggregateRoot

## Overview

Starting with version 0.2.0, the `AutoDispatchAggregate` class has been removed from the TypeScript Event Sourcing SDK. Its functionality has been merged directly into the `AggregateRoot` class, simplifying the inheritance hierarchy.

**This is a breaking change that requires updating your imports.**

## Why This Change?

The `AutoDispatchAggregate` class represented unnecessary duplication in our aggregate hierarchy. Since all production aggregates should extend `AggregateRoot` (which includes automatic event dispatching, command handlers, and decorators), having a middle layer added complexity without providing meaningful flexibility.

For the full architectural rationale, see [ADR-005: Remove AutoDispatchAggregate Duplication](/adrs/ADR-005-remove-autodispatch-aggregate).

## What Changed

### Before (3-level hierarchy)
```
BaseAggregate → AutoDispatchAggregate → AggregateRoot
```

### After (2-level hierarchy)
```
BaseAggregate → AggregateRoot
```

**All functionality is preserved** - we simply merged `AutoDispatchAggregate` into `AggregateRoot`.

## Migration Steps

### Step 1: Update Imports

Replace all imports of `AutoDispatchAggregate` with `AggregateRoot`:

```typescript
// ❌ Before
import { AutoDispatchAggregate } from '@event-sourcing-platform/typescript';

// ✅ After
import { AggregateRoot } from '@event-sourcing-platform/typescript';
```

### Step 2: Update Class Declarations

Change your aggregate class declarations:

```typescript
// ❌ Before
export class TaskAggregate extends AutoDispatchAggregate<TaskEvent> {
  // ... your code
}

// ✅ After
export class TaskAggregate extends AggregateRoot<TaskEvent> {
  // ... your code (no other changes needed)
}
```

### Step 3: Verify Tests

Run your tests to ensure everything still works:

```bash
npm test
```

**That's it!** No behavioral changes are required - just update the class name.

## Complete Example

Here's a complete before/after example:

### Before

```typescript
import {
  AutoDispatchAggregate,
  BaseDomainEvent,
  EventSourcingHandler,
} from '@event-sourcing-platform/typescript';

class TaskCreatedEvent extends BaseDomainEvent {
  readonly eventType = 'TaskCreated' as const;
  readonly schemaVersion = 1 as const;
  
  constructor(
    public readonly taskId: string,
    public readonly title: string
  ) {
    super();
  }
}

class TaskAggregate extends AutoDispatchAggregate<TaskCreatedEvent> {
  private title: string | null = null;

  getAggregateType(): string {
    return 'Task';
  }

  createTask(taskId: string, title: string): void {
    if (this.id !== null) {
      throw new Error('Task already exists');
    }
    this.initialize(taskId);
    this.raiseEvent(new TaskCreatedEvent(taskId, title));
  }

  @EventSourcingHandler('TaskCreated')
  private onTaskCreated(event: TaskCreatedEvent): void {
    this.title = event.title;
  }

  getTitle(): string | null {
    return this.title;
  }
}
```

### After

```typescript
import {
  AggregateRoot,  // ← Only change: import name
  BaseDomainEvent,
  EventSourcingHandler,
} from '@event-sourcing-platform/typescript';

class TaskCreatedEvent extends BaseDomainEvent {
  readonly eventType = 'TaskCreated' as const;
  readonly schemaVersion = 1 as const;
  
  constructor(
    public readonly taskId: string,
    public readonly title: string
  ) {
    super();
  }
}

class TaskAggregate extends AggregateRoot<TaskCreatedEvent> {  // ← Only change: class name
  private title: string | null = null;

  getAggregateType(): string {
    return 'Task';
  }

  createTask(taskId: string, title: string): void {
    if (this.id !== null) {
      throw new Error('Task already exists');
    }
    this.initialize(taskId);
    this.raiseEvent(new TaskCreatedEvent(taskId, title));
  }

  @EventSourcingHandler('TaskCreated')
  private onTaskCreated(event: TaskCreatedEvent): void {
    this.title = event.title;
  }

  getTitle(): string | null {
    return this.title;
  }
}
```

## VSA Configuration

If you're using the Vertical Slice Architecture (VSA) CLI, update your `vsa.yaml` configuration:

```yaml
# ❌ Before
vsa:
  framework:
    name: event-sourcing-platform
    base_types:
      aggregate:
        import: "@event-sourcing-platform/typescript"
        class: "AutoDispatchAggregate"

# ✅ After
vsa:
  framework:
    name: event-sourcing-platform
    base_types:
      aggregate:
        import: "@event-sourcing-platform/typescript"
        class: "AggregateRoot"
```

## Advanced Users: BaseAggregate

If you need manual event handling without decorators, you can still use `BaseAggregate`:

```typescript
import { BaseAggregate } from '@event-sourcing-platform/typescript';

class LowLevelAggregate extends BaseAggregate<MyEvent> {
  getAggregateType(): string {
    return 'LowLevel';
  }

  applyEvent(event: MyEvent): void {
    // Manual event dispatching
    if (event.eventType === 'MyEvent') {
      this.onMyEvent(event);
    }
  }

  private onMyEvent(event: MyEvent): void {
    // Handle event manually
  }
}
```

However, **this is rarely needed**. Most use cases should use `AggregateRoot`.

## Common Pitfalls

### ❌ Mixing Old and New Code

Don't mix `AutoDispatchAggregate` and `AggregateRoot` in the same codebase:

```typescript
// ❌ Bad - will cause import errors
import { AutoDispatchAggregate } from '@event-sourcing-platform/typescript';
class TaskAggregate extends AutoDispatchAggregate<TaskEvent> { }
```

### ✅ Solution

Update **all** aggregates at once using find-and-replace:
- Find: `AutoDispatchAggregate`
- Replace with: `AggregateRoot`

### ❌ Not Updating VSA Config

If you use VSA CLI and forget to update `vsa.yaml`, generated code will use the wrong class name.

### ✅ Solution

Update your `vsa.yaml` configuration file as shown above, then regenerate any templates.

## FAQ

### Q: Will my existing aggregates still work?

**A:** Yes, if you update the import and class name. The behavior is identical.

### Q: Do I need to migrate my persisted events?

**A:** No. Events are stored as data - the aggregate class structure doesn't affect them.

### Q: What if I have custom decorators on AutoDispatchAggregate?

**A:** Move them to `AggregateRoot`. The decorator infrastructure is unchanged.

### Q: Can I keep using AutoDispatchAggregate?

**A:** No, it has been completely removed. You must migrate to `AggregateRoot`.

### Q: What version introduced this change?

**A:** Version 0.2.0 of `@event-sourcing-platform/typescript`.

## Automated Migration

If you have many files to update, use this bash one-liner:

```bash
# Find and replace AutoDispatchAggregate with AggregateRoot in all .ts files
find . -name "*.ts" -type f -exec sed -i '' 's/AutoDispatchAggregate/AggregateRoot/g' {} +
```

**Note:** Review changes before committing!

## Need Help?

If you encounter issues during migration:

1. Check the [ADR-005](/adrs/ADR-005-remove-autodispatch-aggregate) for design rationale
2. Review the [AggregateRoot API documentation](/event-sourcing/sdks/api-reference)
3. See [examples](/event-sourcing/sdks/typescript/typescript-sdk) for working code
4. Open an issue on GitHub if you're stuck

## Version Compatibility

| SDK Version | AutoDispatchAggregate | AggregateRoot | Migration Required |
|-------------|----------------------|---------------|-------------------|
| 0.1.x       | ✅ Available          | ✅ Available   | No                |
| 0.2.x+      | ❌ Removed            | ✅ Available   | **Yes**           |

---

**Summary:** Replace `AutoDispatchAggregate` with `AggregateRoot` in imports and class declarations. No other changes needed!

