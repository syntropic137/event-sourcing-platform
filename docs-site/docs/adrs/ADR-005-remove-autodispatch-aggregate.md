# ADR-005: Remove AutoDispatchAggregate Duplication

**Status:** Accepted  
**Date:** 2025-11-05  
**Deciders:** Architecture Team  
**Related ADRs:** ADR-004 (Command Handlers in Aggregates)

## Context and Problem Statement

Our aggregate inheritance hierarchy currently has three levels:

```
BaseAggregate (low-level, manual event handling)
    ↓
AutoDispatchAggregate (automatic event dispatch)
    ↓
AggregateRoot (production-ready with command handlers)
```

The `AutoDispatchAggregate` class represents unnecessary duplication. It serves as a middle layer that only adds automatic event dispatching, but:

1. **Single Production Path**: All documentation and examples point to `AggregateRoot` as "the main class that aggregates should extend"
2. **No Real Use Case**: There's no practical scenario where users need `AutoDispatchAggregate` without also wanting `AggregateRoot`'s command handler infrastructure
3. **Complexity Without Benefit**: The middle layer adds cognitive overhead without providing meaningful flexibility

Should we keep this three-level hierarchy or simplify it?

## Decision Drivers

- **Simplicity**: Fewer abstractions = easier to understand
- **Documentation**: One clear production path for users
- **Maintenance**: Less code to maintain and test
- **API Clarity**: Remove confusion about which class to extend
- **DDD Principles**: Aggregates should be complete units (state + behavior + command handling)

## Considered Options

### Option A: Keep Three-Level Hierarchy (Status Quo)

**Structure:**
```typescript
BaseAggregate          // Manual event handling
AutoDispatchAggregate  // + Auto event dispatch
AggregateRoot          // + Command handlers
```

**Pros:**
- No breaking changes
- Maximum flexibility
- Separation of concerns

**Cons:**
- Confusing to users (which to extend?)
- Extra layer with minimal value
- More code to maintain
- Documentation must explain all three

### Option B: Remove AutoDispatchAggregate (Merge into AggregateRoot)

**Structure:**
```typescript
BaseAggregate    // Manual event handling (advanced users)
AggregateRoot    // Auto-dispatch + Command handlers (production)
```

**Pros:**
- Clear production path: extend `AggregateRoot`
- Simpler mental model
- Less code to maintain
- Clearer documentation
- Still flexible (BaseAggregate for edge cases)

**Cons:**
- Breaking change for external users
- Must migrate existing code

### Option C: Remove All Abstraction (Single Class)

Single `Aggregate` class with all features.

**Pros:**
- Simplest possible API
- No inheritance needed

**Cons:**
- No flexibility for advanced users
- Forces opinionated approach
- Loses DDD modularity

## Decision Outcome

**Chosen option: Option B - Remove AutoDispatchAggregate**

Merge `AutoDispatchAggregate` functionality directly into `AggregateRoot`, creating a clean two-level hierarchy:

1. **`BaseAggregate`** - Low-level class for advanced users who need manual event handling
2. **`AggregateRoot`** - Production-ready class with auto-dispatch + command handlers + decorators

### Rationale

1. **Single Obvious Choice**: Users should extend `AggregateRoot` in 99% of cases
2. **Reduced Complexity**: Eliminates middle layer that serves no practical purpose
3. **Better Documentation**: One production class to document and teach
4. **Maintains Flexibility**: `BaseAggregate` remains for edge cases
5. **Follows DDD**: Aggregates should be complete units with all production features

### Migration Path

The migration is trivial for external consumers:

```diff
- import { AutoDispatchAggregate } from '@event-sourcing-platform/typescript';
- class MyAggregate extends AutoDispatchAggregate<MyEvent> {
+ import { AggregateRoot } from '@event-sourcing-platform/typescript';
+ class MyAggregate extends AggregateRoot<MyEvent> {
```

No behavioral changes - all existing functionality is preserved.

## Implementation Details

### Before (Three Levels)

```typescript
// Level 1: Manual event handling
export abstract class BaseAggregate<TEvent> {
  protected applyChange(event: TEvent): void {
    this.uncommittedEvents.push(event);
    // Manual dispatch required
  }
}

// Level 2: Automatic event dispatch
export abstract class AutoDispatchAggregate<TEvent> extends BaseAggregate<TEvent> {
  protected apply(event: TEvent): void {
    this.applyChange(event);
    this.applyEvent(event); // Auto-dispatch to handlers
  }

  protected applyEvent(event: TEvent): void {
    // Find and call @EventSourcingHandler methods
  }
}

// Level 3: Command handlers
export abstract class AggregateRoot<TEvent> extends AutoDispatchAggregate<TEvent> {
  protected handleCommand<TCommand>(command: TCommand): void {
    // Command routing logic
  }
}
```

### After (Two Levels)

```typescript
// Level 1: Manual event handling
export abstract class BaseAggregate<TEvent> {
  protected applyChange(event: TEvent): void {
    this.uncommittedEvents.push(event);
    // Manual dispatch required
  }
}

// Level 2: Production-ready (auto-dispatch + commands)
export abstract class AggregateRoot<TEvent> extends BaseAggregate<TEvent> {
  // Auto-dispatch functionality (merged from AutoDispatchAggregate)
  protected apply(event: TEvent): void {
    this.applyChange(event);
    this.applyEvent(event);
  }

  protected applyEvent(event: TEvent): void {
    // Find and call @EventSourcingHandler methods
  }

  // Command handling (existing)
  protected handleCommand<TCommand>(command: TCommand): void {
    // Command routing logic
  }
}
```

### Usage Pattern (Unchanged)

```typescript
@Aggregate('Task')
export class TaskAggregate extends AggregateRoot<TaskEvent> {
  private title: string | null = null;

  @CommandHandler('CreateTaskCommand')
  createTask(command: CreateTaskCommand): void {
    if (this.id !== null) {
      throw new Error('Task already exists');
    }
    
    this.apply(new TaskCreatedEvent(command.aggregateId, command.title));
  }

  @EventSourcingHandler('TaskCreated')
  private onTaskCreated(event: TaskCreatedEvent): void {
    this.initialize(event.id);
    this.title = event.title;
  }
}
```

## Consequences

### Positive

✅ **Simplified API**
- Users have one obvious choice: `AggregateRoot`
- Clearer documentation and examples
- Less cognitive overhead

✅ **Reduced Code Complexity**
- One less class to maintain
- Fewer tests needed
- Simpler inheritance hierarchy

✅ **Better Developer Experience**
- No confusion about which class to extend
- Faster onboarding
- Less to learn

✅ **Maintained Flexibility**
- `BaseAggregate` still available for edge cases
- No functionality removed
- All features preserved

### Negative

⚠️ **Breaking Change**
- External consumers using `AutoDispatchAggregate` must update imports
- Requires version bump (minor or major)
- Migration guide needed

⚠️ **Documentation Updates**
- All docs mentioning `AutoDispatchAggregate` must be updated
- Examples need refactoring
- Migration guide required

### Neutral

🔄 **Two-Level Hierarchy**
- Still more than one class (not a single-class API)
- Advanced users can still use `BaseAggregate`
- Appropriate level of abstraction

## Validation

### Success Criteria

- [ ] All tests pass
- [ ] All examples use `AggregateRoot`
- [ ] No references to `AutoDispatchAggregate` remain (except in migration docs)
- [ ] Documentation is clear and accurate
- [ ] VSA CLI generates code with `AggregateRoot`
- [ ] Migration guide is complete

### Affected Components

1. **Core Library** (event-sourcing/typescript)
   - `src/core/aggregate.ts` - Remove class, merge functionality
   - `src/index.ts` - Remove from exports

2. **Tests**
   - `tests/helpers/order-aggregate.ts` - Update to `AggregateRoot`

3. **Documentation**
   - `event-sourcing/typescript/README.md` - Update examples

4. **Examples** (7 projects)
   - All TypeScript examples using `AutoDispatchAggregate`

5. **VSA CLI**
   - `vsa/vsa-cli/src/templates/typescript.rs` - Update templates
   - `vsa/vsa-cli/src/commands/init.rs` - Update config templates

6. **VSA Examples**
   - `vsa/examples/*/vsa.yaml` - Update configurations

7. **Documentation Site**
   - Multiple markdown files referencing `AutoDispatchAggregate`

## References

### Internal Documentation
- **Project Plan**: `/PROJECT-PLAN_20251105_remove-autodispatch-aggregate-duplication.md`
- **Related ADR**: `/docs-site/docs/adrs/ADR-004-command-handlers-in-aggregates.md`
- **Library README**: `/event-sourcing/typescript/README.md`

### Design Principles
- **YAGNI** (You Aren't Gonna Need It): Don't add abstraction until needed
- **KISS** (Keep It Simple, Stupid): Simpler is better
- **Single Responsibility**: Each class should have one clear purpose
- **DDD Aggregates**: Complete units with state + behavior

## Review History

| Date | Reviewer | Status | Comments |
|------|----------|--------|----------|
| 2025-11-05 | Architecture Team | Accepted | Simplification makes sense |
| 2025-11-05 | Lead Developer | Accepted | Easy migration path |
| 2025-11-05 | Documentation Team | Accepted | Will improve clarity |

## Approval

**Decision Status:** ✅ **ACCEPTED**

**Approved by:** Architecture Team  
**Date:** 2025-11-05  
**Effective Date:** 2025-11-05  
**Review Date:** 2026-02-05 (3 months)

---

## Migration Guide

### For Library Users

If you're using `AutoDispatchAggregate` directly:

```typescript
// Before
import { AutoDispatchAggregate } from '@event-sourcing-platform/typescript';

@Aggregate('MyDomain')
class MyAggregate extends AutoDispatchAggregate<MyEvent> {
  // ... your code
}

// After
import { AggregateRoot } from '@event-sourcing-platform/typescript';

@Aggregate('MyDomain')
class MyAggregate extends AggregateRoot<MyEvent> {
  // ... your code (no changes needed)
}
```

**That's it!** No behavioral changes, just a different base class name.

### For Advanced Users

If you need manual event handling without decorators:

```typescript
// Use BaseAggregate instead
import { BaseAggregate } from '@event-sourcing-platform/typescript';

class LowLevelAggregate extends BaseAggregate<MyEvent> {
  protected handleEvent(event: MyEvent): void {
    // Manual event handling
    this.applyChange(event);
    
    // Manual dispatching
    if (event.eventType === 'MyEvent') {
      this.onMyEvent(event);
    }
  }
}
```

---

*This ADR documents the removal of `AutoDispatchAggregate` to simplify the aggregate inheritance hierarchy. All new code should extend `AggregateRoot` for production use.*

