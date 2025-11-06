---
sidebar_position: 4
---

# Convention Over Configuration

Learn how VSA uses naming conventions and structure patterns to minimize configuration.

## What is Convention Over Configuration?

**Convention Over Configuration** is a software design paradigm where the framework provides sensible defaults based on naming conventions and structure patterns, reducing the need for explicit configuration.

###Philosophy

```yaml
# ❌ Configuration-heavy approach
vsa:
  patterns:
    command: "**/*Command.ts"
    event: "**/*Event.ts"
    handler: "**/*Handler.ts"
    aggregate: "**/*Aggregate.ts"
    test: "**/*.test.ts"
  validation:
    require_command_in_operation: true
    require_event_per_command: true
    # ... 50 more configuration options

# ✅ Convention-based approach (VSA)
vsa:
  version: 1
  language: typescript
  root: src/contexts
  # That's it! Everything else is convention
```

**Benefits:**
- ✅ Less configuration to write and maintain
- ✅ Faster onboarding (learn conventions once)
- ✅ Consistency across projects
- ✅ Better tooling support (predictable structure)
- ✅ Grep-friendly code

## File Naming Conventions

VSA recognizes files by their suffix:

| Pattern | Purpose | Example |
|---------|---------|---------|
| `*Command.{ext}` | Command intent | `PlaceOrderCommand.ts` |
| `*Event.{ext}` | Domain event | `OrderPlacedEvent.ts` |
| `*Handler.{ext}` | Command/event handler | `PlaceOrderHandler.ts` |
| `*Aggregate.{ext}` | Domain aggregate | `OrderAggregate.ts` |
| `*Query.{ext}` | Query | `GetOrderQuery.ts` |
| `*Projection.{ext}` | Read model projection | `OrderSummaryProjection.ts` |
| `*.test.{ext}` | Tests | `PlaceOrder.test.ts` |
| `*.handler.{ext}` | Integration event subscriber | `ProductAdded.handler.ts` |

### Why These Conventions?

#### 1. Grep-Friendly

```bash
# Find all commands
git grep "Command" --name-only

# Find specific command
git grep "PlaceOrderCommand"

# Find all tests for a feature
git grep "PlaceOrder.test"
```

#### 2. Self-Documenting

```typescript
// File name tells you exactly what it is
PlaceOrderCommand.ts      // ← Obviously a command
OrderPlacedEvent.ts       // ← Obviously an event
PlaceOrderHandler.ts      // ← Obviously handles the command
PlaceOrder.test.ts        // ← Obviously tests the feature
```

#### 3. IDE Support

Modern IDEs can:
- Autocomplete based on file patterns
- Navigate between related files
- Group related files together
- Provide context-aware suggestions

### Examples

#### ✅ Good: Follows Conventions

```
place-order/
├── PlaceOrderCommand.ts      ← Clear: This is a command
├── OrderPlacedEvent.ts        ← Clear: This is an event
├── PlaceOrderHandler.ts       ← Clear: This handles the command
├── OrderAggregate.ts          ← Clear: This is the aggregate
└── PlaceOrder.test.ts         ← Clear: These are the tests
```

#### ❌ Bad: Generic Names

```
place-order/
├── command.ts        ← Which command?
├── event.ts          ← Which event?
├── handler.ts        ← Handles what?
├── aggregate.ts      ← Which aggregate?
└── test.ts           ← Tests what?
```

## Folder Structure Conventions

### Feature Detection

VSA automatically detects features by finding folders containing `*Command.*` files:

```
contexts/
  orders/
    place-order/          ← Feature (has PlaceOrderCommand.ts)
      PlaceOrderCommand.ts
      OrderPlacedEvent.ts
      PlaceOrderHandler.ts
```

**Rules:**
- Folder with `*Command.*` = Feature
- No explicit configuration needed
- Unlimited nesting depth allowed

### Special Folders

Folders with underscore prefix have special meaning:

| Folder | Purpose | Contents |
|--------|---------|----------|
| `_subscribers/` | Integration event handlers | `*.handler.{ext}` files |
| `_shared/` | Context-internal shared code | Aggregates, utilities, etc. |
| `_queries/` | Query features | Read-side operations |

#### Example Structure

```
contexts/
  orders/
    place-order/              ← Regular feature
    cancel-order/             ← Regular feature
    _subscribers/             ← Special: Event handlers
      ProductAdded.handler.ts
      StockAdjusted.handler.ts
    _shared/                  ← Special: Shared within context
      OrderAggregate.ts
      OrderValidator.ts
    _queries/                 ← Special: Query operations
      get-order/
      list-orders/
```

## Minimal Configuration

### Basic `vsa.yaml`

```yaml
version: 1
language: typescript
root: src/contexts
```

**That's enough to get started!**

### With Bounded Contexts

```yaml
version: 1
language: typescript
root: src/contexts

bounded_contexts:
  - name: orders
    description: Order processing
  
  - name: inventory
    description: Stock management
```

### With Framework Integration

```yaml
version: 1
language: typescript
root: src/contexts

framework:
  name: event-sourcing-platform
  aggregate_class: AggregateRoot
  aggregate_import: "@event-sourcing-platform/typescript"

bounded_contexts:
  - name: orders
    publishes: [OrderPlaced, OrderCancelled]
    subscribes: [ProductAdded, StockAdjusted]
```

### With Validation Rules

```yaml
version: 1
language: typescript
root: src/contexts

validation:
  require_tests: true      # Every feature must have tests
  require_handler: true    # Every command needs a handler
  require_aggregate: false # Aggregates are optional
```

## Convention Patterns

### Pattern 1: Command-Event Pairing

**Convention:** Every command produces at least one event.

```
place-order/
├── PlaceOrderCommand.ts    → OrderPlacedEvent.ts
└── OrderPlacedEvent.ts
```

**VSA validates:**
```bash
❌ Command without event
   Feature: place-order/
   Found: PlaceOrderCommand.ts
   Missing: OrderPlacedEvent.ts
   
   💡 Create event or add to exceptions
```

### Pattern 2: Handler Naming

**Convention:** Handler name matches command/query name.

```
✅ Correct:
PlaceOrderCommand.ts
PlaceOrderHandler.ts    ← Handles PlaceOrderCommand

❌ Incorrect:
PlaceOrderCommand.ts
OrderHandler.ts         ← Not clear what it handles
```

### Pattern 3: Test Naming

**Convention:** Tests named after the feature.

```
✅ Correct:
place-order/
  PlaceOrder.test.ts    ← Tests place-order feature

❌ Incorrect:
place-order/
  orders.test.ts        ← Too generic
  test.ts               ← Way too generic
```

### Pattern 4: Integration Event Location

**Convention:** Integration events always in `_shared/integration-events/{publisher}/`

```
✅ Correct:
_shared/integration-events/
  orders/
    OrderPlaced.ts

❌ Incorrect:
contexts/orders/events/
  OrderPlaced.ts
```

## Validation Rules

VSA automatically validates conventions:

### Rule 1: File Naming

```bash
$ vsa validate

❌ Invalid file name
   File: contexts/orders/place-order/order-command.ts
   Expected: *Command.ts pattern (e.g., PlaceOrderCommand.ts)
   
   💡 Rename to: PlaceOrderCommand.ts
```

### Rule 2: Required Files

```bash
❌ Missing required files
   Feature: place-order/
   Found: PlaceOrderCommand.ts
   Missing: 
     - PlaceOrderHandler.ts (required by validation.require_handler)
     - PlaceOrder.test.ts (required by validation.require_tests)
```

### Rule 3: Special Folders

```bash
❌ Invalid location
   File: contexts/orders/ProductAdded.handler.ts
   Expected: contexts/orders/_subscribers/ProductAdded.handler.ts
   
   💡 Event handlers must be in _subscribers/ folder
```

### Rule 4: Boundary Violations

```bash
❌ Boundary violation
   File: contexts/orders/place-order/PlaceOrderHandler.ts:5
   
   import { GetProductQuery } from '../../../catalog/queries/get-product';
   
   Direct cross-context import (orders → catalog)
   
   💡 Use integration events instead
```

## Escape Hatches

Sometimes you need to deviate from conventions. VSA provides escape hatches:

### Custom Patterns (Future)

```yaml
# Override patterns if needed
patterns:
  command: "*Cmd.ts"        # Instead of *Command.ts
  event: "*Evt.ts"          # Instead of *Event.ts
  handler: "*Hdlr.ts"       # Instead of *Handler.ts
```

### Exclusions

```yaml
# Exclude specific paths from validation
validation:
  exclude:
    - "legacy/**"
    - "external/**"
    - "**/*.generated.ts"
```

### Per-Context Overrides

```yaml
bounded_contexts:
  - name: orders
    validation:
      require_aggregate: true   # Override global setting
      
  - name: notifications
    validation:
      require_aggregate: false  # Different for this context
```

## Benefits of Conventions

### 1. Faster Development

```bash
# No configuration needed, just follow conventions
vsa generate orders place-order

# VSA knows what to create based on conventions
✅ Created PlaceOrderCommand.ts
✅ Created OrderPlacedEvent.ts
✅ Created PlaceOrderHandler.ts
✅ Created PlaceOrder.test.ts
```

### 2. Consistency

```
Project A:        Project B:        Project C:
place-order/      add-product/      register-user/
├── *Command      ├── *Command      ├── *Command
├── *Event        ├── *Event        ├── *Event
├── *Handler      ├── *Handler      ├── *Handler
└── *.test        └── *.test        └── *.test

All follow the same pattern!
```

### 3. Onboarding

```typescript
// New developer sees this structure
place-order/
├── PlaceOrderCommand.ts
├── OrderPlacedEvent.ts
├── PlaceOrderHandler.ts
└── PlaceOrder.test.ts

// Immediately understands:
// - This is a feature for placing orders
// - Command defines the input
// - Event defines what happened
// - Handler contains the logic
// - Tests verify it works
```

### 4. Tooling

```bash
# Find all features in orders context
find contexts/orders -name "*Command.ts"

# Find all event handlers
find contexts -path "*/_subscribers/*.handler.ts"

# Find all tests
find contexts -name "*.test.ts"

# All work because of consistent naming!
```

## Real-World Example

### Without Conventions

```
src/
├── modules/
│   ├── order-processing/
│   │   ├── commands/
│   │   │   └── place.ts            ← Where is it?
│   │   ├── events/
│   │   │   └── placed.ts           ← Hard to search
│   │   ├── handlers/
│   │   │   └── order-placer.ts     ← Inconsistent naming
│   │   └── tests/
│   │       └── placing.spec.ts     ← What does this test?
```

**Problems:**
- Hard to find related files
- Inconsistent naming
- Need configuration to locate files
- Difficult to search

### With Conventions

```
contexts/
  orders/
    place-order/
      PlaceOrderCommand.ts       ← Obvious what it is
      OrderPlacedEvent.ts        ← Obvious what it is
      PlaceOrderHandler.ts       ← Obvious what it is
      PlaceOrder.test.ts         ← Obvious what it is
```

**Benefits:**
- Everything related is together
- Consistent naming
- No configuration needed
- Easy to search: `git grep "PlaceOrder"`

## Comparison with Other Approaches

| Aspect | Configuration-Heavy | Convention-Based (VSA) |
|--------|---------------------|------------------------|
| **Setup Time** | Slow (write config) | Fast (follow conventions) |
| **Configuration** | 100+ lines of YAML | 5-10 lines of YAML |
| **Learning Curve** | Steep (learn config) | Gentle (learn conventions) |
| **Consistency** | Varies by config | Always consistent |
| **Flexibility** | Very flexible | Flexible with escape hatches |
| **Maintainability** | Config can drift | Enforced by validation |

## Best Practices

### ✅ Do's

- Follow naming conventions consistently
- Use special folders (`_subscribers/`, `_shared/`) appropriately
- Keep configuration minimal
- Let VSA validate your structure
- Use descriptive feature names (kebab-case)

### ❌ Don'ts

- Don't use generic file names (`command.ts`, `handler.ts`)
- Don't fight conventions without good reason
- Don't add configuration until you need it
- Don't skip validation
- Don't create your own folder conventions

## Adopting Conventions

### For New Projects

```bash
# 1. Initialize with minimal config
vsa init --language typescript

# 2. Generate features
vsa generate orders place-order

# 3. Follow the generated structure
# VSA creates files following conventions

# 4. Validate regularly
vsa validate --watch
```

### For Existing Projects

```bash
# 1. Start with one feature
# Refactor to follow conventions

# 2. Validate
vsa validate

# 3. Fix violations one by one
# Rename files, move to correct locations

# 4. Repeat for other features
# Gradually adopt conventions
```

## CLI Support

### Generate with Conventions

```bash
# VSA automatically follows conventions
$ vsa generate orders place-order

✅ Created contexts/orders/place-order/PlaceOrderCommand.ts
✅ Created contexts/orders/place-order/OrderPlacedEvent.ts
✅ Created contexts/orders/place-order/PlaceOrderHandler.ts
✅ Created contexts/orders/place-order/PlaceOrder.test.ts

All files follow naming conventions!
```

### Validate Conventions

```bash
# Check if your code follows conventions
$ vsa validate

✅ orders/place-order
   ├─ PlaceOrderCommand.ts ✓
   ├─ OrderPlacedEvent.ts ✓
   ├─ PlaceOrderHandler.ts ✓
   └─ PlaceOrder.test.ts ✓

Summary: 1 feature validated, 0 violations
```

## Next Steps

- **[CLI Commands](../cli/commands)** - Generate code following conventions
- **[Validation Rules](../cli/validation-rules)** - All validation rules explained
- **[Examples](../examples/overview)** - See conventions in action

## Resources

- [Convention Over Configuration - Wikipedia](https://en.wikipedia.org/wiki/Convention_over_configuration)
- [Ruby on Rails - Convention Over Configuration](https://rubyonrails.org/doctrine#convention-over-configuration)
- [Next.js File Conventions](https://nextjs.org/docs/app/api-reference/file-conventions)

---

**Ready to build?** Follow the conventions and let VSA guide you with [CLI Commands](../cli/commands).

