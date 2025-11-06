# ADR-002: Convention Over Configuration

**Status:** Accepted  
**Date:** 2025-11-05  
**Deciders:** Architecture Team  
**Context:** VSA configuration philosophy

## Context and Problem Statement

How should VSA balance flexibility with simplicity? Should it be:
- Highly configurable (flexible but complex)?
- Convention-based (simple but opinionated)?
- Hybrid approach?

## Decision Drivers

- **Developer Experience**: Easy to get started
- **Consistency**: Teams should structure code similarly
- **Flexibility**: Allow customization when needed
- **Maintenance**: Less configuration = less to maintain
- **Searchability**: Grep-friendly naming is crucial

## Considered Options

### Option A: Minimal Config (Convention-First)
```yaml
vsa:
  root: ./src/contexts
  language: typescript
# Everything else is convention
```

### Option B: Flexible Config (Configuration-First)
```yaml
vsa:
  root: ./src/contexts
  patterns:
    command: "**/*Command.ts"
    event: "**/*Event.ts"
    # ... extensive configuration
```

### Option C: Discovery-Based (Auto-Detect)
Automatically detect structure, minimal config

## Decision Outcome

**Chosen option: Option A - Convention Over Configuration (with escape hatches)**

### Core Conventions

#### File Naming
```
*Command.{ext}     → Command
*Event.{ext}       → Domain Event (internal)
*Handler.{ext}     → Command/Query Handler
*Aggregate.{ext}   → Aggregate (optional)
*Query.{ext}       → Query
*Projection.{ext}  → Read Model Projection
*.test.{ext}       → Tests
```

**Rationale**: Grep-friendly, semantically clear, follows common patterns

#### Folder Structure
```
contexts/
  {context}/
    {feature}/
      {operation}/        ← Contains *Command.{ext}
        ...files...
```

**Rationale**: Clear hierarchy, bounded contexts explicit, unlimited nesting

#### Special Folders
```
_subscribers/    → Integration event handlers
_shared/         → Context-internal shared code
```

**Rationale**: Underscore prefix indicates "special", clear purpose

### Minimal Configuration

```yaml
vsa:
  version: 1
  root: ./src/contexts
  language: typescript
  
  # Optional: Framework integration
  framework: event-sourcing-platform
  
  # Optional: Bounded contexts
  bounded_contexts:
    - name: warehouse
      path: warehouse/
      publishes: [ProductStockChanged]
      subscribes: [OrderPlaced]
```

### Escape Hatches (for future)

```yaml
# Override conventions if absolutely needed
suffixes:
  command: Command       # Default
  event: Event           # Default
  handler: Handler       # Default
```

### Positive Consequences

- **Fast Onboarding**: New developers understand structure immediately
- **Consistency**: All projects look similar
- **Less Configuration**: Less to learn, less to maintain
- **Grep-Friendly**: `git grep "CreateProductCommand"` works perfectly
- **Tooling Integration**: IDEs can autocomplete better

### Negative Consequences

- **Less Flexible**: Hard to adapt unusual codebases
- **Migration Harder**: Existing projects need restructuring
- **Opinionated**: Some teams may disagree with conventions

## Mitigation Strategies

- **Migration**: Future tool to restructure existing code
- **Documentation**: Clear explanation of conventions and rationale
- **Validation**: Helpful error messages guide developers
- **Community**: Gather feedback, iterate on conventions if needed

## Examples

### Good: Convention-Following Structure
```
contexts/
  warehouse/
    products/
      create-product/
        CreateProductCommand.ts       ← Clear, searchable
        ProductCreatedEvent.ts        ← Clear, searchable
        CreateProductHandler.ts       ← Clear, searchable
        CreateProduct.test.ts         ← Clear, searchable
```

### Bad: Generic Names (What We're Avoiding)
```
contexts/
  warehouse/
    products/
      create-product/
        command.ts      ← Hard to search for
        event.ts        ← Which event?
        handler.ts      ← Which handler?
        test.ts         ← Generic
```

## Inspiration

- **Next.js App Router**: File-system based conventions
- **Ruby on Rails**: Convention over configuration philosophy
- **.NET Vertical Slices**: Feature folder naming patterns
- **Oskar Dudycz**: Event sourcing structure conventions

## Links

- [Next.js File Conventions](https://nextjs.org/docs/app/api-reference/file-conventions)
- [Oskar Dudycz - How to Slice the Codebase](https://event-driven.io/en/how_to_slice_the_codebase_effectively/)
- [Convention Over Configuration](https://en.wikipedia.org/wiki/Convention_over_configuration)

