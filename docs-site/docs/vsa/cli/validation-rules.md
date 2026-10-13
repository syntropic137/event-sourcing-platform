---
sidebar_position: 3
---

# Validation Rules

Complete reference for all VSA validation rules.

## Overview

VSA enforces architectural rules through automatic validation. This ensures consistency, maintainability, and adherence to conventions across your codebase.

## Rule Categories

1. **File Naming** - Conventional suffixes
2. **Required Files** - Essential files per feature
3. **Bounded Context Rules** - No boundary violations
4. **Special Folders** - Correct usage of `_subscribers/`, `_shared/`
5. **Integration Events** - Single source of truth

## File Naming Rules

### Rule: Command Suffix

**What:** Command files must end with `Command.{ext}`

```
✅ Correct:
PlaceOrderCommand.ts
CreateUserCommand.py

❌ Incorrect:
PlaceOrderCmd.ts
place_order_command.ts
Order.ts
```

**Validation:**
```bash
❌ Invalid file name
   File: contexts/orders/place-order/PlaceOrder.ts
   Expected: *Command.ts pattern
   
   💡 Rename to: PlaceOrderCommand.ts
```

### Rule: Event Suffix

**What:** Event files must end with `Event.{ext}`

```
✅ Correct:
OrderPlacedEvent.ts
UserCreatedEvent.py

❌ Incorrect:
OrderPlaced.ts
order_placed_event.ts
```

### Rule: Handler Suffix

**What:** Handler files must end with `Handler.{ext}`

```
✅ Correct:
PlaceOrderHandler.ts
CreateUserHandler.py

❌ Incorrect:
PlaceOrderService.ts
OrderHandler.ts (too generic)
```

### Rule: Test Suffix

**What:** Test files must end with `.test.{ext}` or `.spec.{ext}`

```
✅ Correct:
PlaceOrder.test.ts
CreateUser.spec.py

❌ Incorrect:
PlaceOrderTests.ts
test_place_order.py (wrong pattern)
```

## Required Files Rules

### Rule: Command Required

**What:** Every feature must have at least one command file

**Validation:**
```bash
❌ Missing command file
   Feature: contexts/orders/place-order/
   Expected: At least one *Command.ts file
```

### Rule: Handler Required

**What:** When `require_handler: true`, every feature needs a handler

**Configuration:**
```yaml
validation:
  require_handler: true  # Default: true
```

**Validation:**
```bash
❌ Missing handler
   Feature: contexts/orders/place-order/
   Found: PlaceOrderCommand.ts
   Missing: PlaceOrderHandler.ts
   
   💡 Required by: validation.require_handler = true
```

### Rule: Tests Required

**What:** When `require_tests: true`, every feature needs tests

**Configuration:**
```yaml
validation:
  require_tests: true  # Default: true
```

**Validation:**
```bash
❌ Missing tests
   Feature: contexts/orders/place-order/
   Expected: PlaceOrder.test.ts or PlaceOrder.spec.ts
   
   💡 Required by: validation.require_tests = true
```

### Rule: Aggregate Required

**What:** When `require_aggregate: true`, every feature needs an aggregate

**Configuration:**
```yaml
validation:
  require_aggregate: false  # Default: false
```

**Validation:**
```bash
❌ Missing aggregate
   Feature: contexts/orders/place-order/
   Expected: *Aggregate.ts file
   
   💡 Required by: validation.require_aggregate = true
```

## Bounded Context Rules

### Rule: No Cross-Context Imports

**What:** Contexts cannot directly import from other contexts

```typescript
// ❌ FORBIDDEN
// contexts/orders/place-order/PlaceOrderHandler.ts
import { GetProductQuery } from '../../../catalog/queries/GetProduct';

// ✅ ALLOWED
// Import from _shared/integration-events
import { ProductAdded } from '../../../_shared/integration-events/catalog';
```

**Validation:**
```bash
❌ Boundary violation
   File: contexts/orders/place-order/PlaceOrderHandler.ts:5
   
   import { GetProductQuery } from '../../../catalog/queries/GetProduct';
   
   Direct cross-context import (orders → catalog)
   
   💡 Use integration events instead:
      1. Catalog publishes ProductAdded event
      2. Orders subscribes and maintains local read model
      3. Query local read model
```

### Rule: Integration Events in Shared

**What:** Integration events must be in `_shared/integration-events/{publisher}/`

```
✅ Correct:
_shared/integration-events/
  catalog/
    ProductAdded.ts
  orders/
    OrderPlaced.ts

❌ Incorrect:
contexts/catalog/events/ProductAdded.ts
contexts/orders/OrderPlaced.ts
```

**Validation:**
```bash
❌ Integration event in wrong location
   File: contexts/catalog/events/ProductAdded.ts
   Should be: _shared/integration-events/catalog/ProductAdded.ts
   
   💡 Integration events must be in _shared/integration-events/
```

### Rule: No Duplicate Events

**What:** Integration events defined exactly once

**Validation:**
```bash
❌ Duplicate event definition
   Event: ProductAdded
   Found in:
     - _shared/integration-events/catalog/ProductAdded.ts ✓
     - contexts/orders/_shared/ProductAdded.ts ✗
   
   💡 Remove duplicate, import from _shared/integration-events/
```

## Special Folders Rules

### Rule: Subscribers in _subscribers/

**What:** Integration event handlers must be in `_subscribers/` folder

```
✅ Correct:
contexts/orders/
  _subscribers/
    ProductAdded.handler.ts

❌ Incorrect:
contexts/orders/
  handlers/
    ProductAdded.handler.ts
```

**Validation:**
```bash
❌ Event handler in wrong location
   File: contexts/orders/handlers/ProductAdded.handler.ts
   Should be: contexts/orders/_subscribers/ProductAdded.handler.ts
```

### Rule: Subscriber Naming

**What:** Subscriber files must match pattern `{EventName}.handler.{ext}`

```
✅ Correct:
ProductAdded.handler.ts
OrderPlaced.handler.py

❌ Incorrect:
ProductAddedHandler.ts
product-added-handler.ts
```

### Rule: Subscribers Match Config

**What:** Subscribed events must have corresponding handlers

**Configuration:**
```yaml
bounded_contexts:
  - name: orders
    subscribes:
      - ProductAdded
      - StockAdjusted
```

**Validation:**
```bash
❌ Missing subscriber handler
   Context: orders
   Event: ProductAdded (in subscribes list)
   Expected: contexts/orders/_subscribers/ProductAdded.handler.ts
   
   💡 Create handler: vsa generate subscriber ProductAdded --context orders
```

### Rule: Publishers Match Config

**What:** Published events must exist in `_shared/integration-events/`

**Configuration:**
```yaml
bounded_contexts:
  - name: catalog
    publishes:
      - ProductAdded
```

**Validation:**
```bash
❌ Published event not found
   Context: catalog
   Event: ProductAdded (in publishes list)
   Expected: _shared/integration-events/catalog/ProductAdded.ts
   
   💡 Create event: vsa generate integration-event ProductAdded --publisher catalog
```

## Slice Thickness Rules

These rules check that a slice does its pattern's job, not only what it may not import. With `architecture: hexagonal-event-sourced-vsa`, slices are found in every context (`<root>/<context>/slices/<slice>/`), their files are read recursively (`internal/` counts), and snake_case role names (`place_order_handler.rs`, `projection.py`) are recognised. Other architectures keep scanning only `<root>/slices/` for now.

### VSA034: Command slice reaches the domain

A `type: command` slice must import the domain layer, where the decision lives (an aggregate's `decide`, or a command handler on the aggregate). A slice that never reaches the domain is deciding on its own.

### VSA035: Command slice reaches a port

A `type: command` slice must import a port (`ports/`), its way to persist what the domain decided, such as the event store.

Both are on by default for `hexagonal-event-sourced-vsa` and off for other architectures. Set either one explicitly to override that:

```yaml
slices:
  command:
    require_domain_call: true   # VSA034
    require_port: true          # VSA035
```

### VSA007: View (query) slice has a projection

A query slice must contain a projection file (`*Projection.*`, `*_projection.*` or `projection.*`). For hexagonal projects this rule now reaches per-context slices.

### Not checked: business invariants in a handler

No rule detects invariants written in a handler. A handler that imports the domain and still validates on its own passes. Telling a domain invariant apart from input parsing or port error mapping needs a reading of the code, not a pattern. A guard-clause count would flag thin handlers that map errors, and miss fat ones written as `match` arms. VSA034, VSA035, the adapter line budget (VSA011) and slice isolation (VSA010, VSA031) are the structural proxies.

## Configuration-Based Rules

### Per-Context Overrides

Override validation for specific contexts:

```yaml
bounded_contexts:
  - name: orders
    validation:
      require_aggregate: true   # Strict for orders
      require_tests: true
  
  - name: notifications
    validation:
      require_aggregate: false  # Relaxed for notifications
      require_tests: false
```

### Global Exclusions

Exclude paths from validation:

```yaml
validation:
  require_tests: true
  exclude:
    - "**/*.generated.ts"
    - "**/legacy/**"
    - "**/temp/**"
```

## Validation Output

### Success

```bash
$ vsa validate

✅ Validating: src/contexts

📦 Context: orders
  ✅ place-order
     ├─ PlaceOrderCommand.ts
     ├─ OrderPlacedEvent.ts
     ├─ PlaceOrderHandler.ts
     └─ PlaceOrder.test.ts

  ✅ cancel-order
     ├─ CancelOrderCommand.ts
     ├─ OrderCancelledEvent.ts
     ├─ CancelOrderHandler.ts
     └─ CancelOrder.test.ts

✅ Summary: 2 features validated, 0 errors
```

### Errors

```bash
$ vsa validate

❌ Validating: src/contexts

📦 Context: orders
  ✅ place-order
  
  ❌ cancel-order
     ├─ CancelOrderCommand.ts
     ├─ OrderCancelledEvent.ts
     └─ ✗ Missing: CancelOrderHandler.ts
     └─ ✗ Missing: CancelOrder.test.ts

📦 Context: inventory
  ❌ adjust-stock
     ├─ ✗ Invalid: adjust-stock-command.ts (should be AdjustStockCommand.ts)

❌ Summary: 2 features validated, 3 errors

💡 Fix these issues:
  1. Create contexts/orders/cancel-order/CancelOrderHandler.ts
  2. Create contexts/orders/cancel-order/CancelOrder.test.ts
  3. Rename contexts/inventory/adjust-stock/adjust-stock-command.ts
     to AdjustStockCommand.ts
```

## Auto-Fix (Future)

Some rules can be auto-fixed:

```bash
$ vsa validate --fix

🔧 Auto-fixing issues...

✅ Renamed: adjust-stock-command.ts → AdjustStockCommand.ts
✅ Created: CancelOrderHandler.ts (from template)
✅ Created: CancelOrder.test.ts (from template)

✅ Fixed 3 issues automatically
```

## Disabling Rules

### Temporary Disable

Use exclusions for temporary exceptions:

```yaml
validation:
  exclude:
    - "**/wip/**"  # Work in progress
```

### Per-Feature Disable

Add `.vsaignore` file in feature folder (future):

```
# .vsaignore
# Temporarily skip validation for this feature
skip: true
reason: "Refactoring in progress"
```

## Best Practices

### ✅ Do's

- Fix violations as they appear
- Use watch mode during development
- Enable all validation in CI/CD
- Document any exclusions
- Review validation regularly

### ❌ Don'ts

- Don't disable all validation
- Don't ignore boundary violations
- Don't commit validation errors
- Don't use exclusions as permanent workarounds

## CI/CD Integration

### GitHub Actions

```yaml
name: VSA Validation

on: [push, pull_request]

jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Install Rust
        uses: actions-rs/toolchain@v1
      - name: Install VSA
        run: cargo install --path vsa/vsa-cli
      - name: Validate
        run: vsa validate
```

### Pre-commit Hook

```bash
#!/bin/bash
# .git/hooks/pre-commit

vsa validate
if [ $? -ne 0 ]; then
  echo "VSA validation failed. Commit aborted."
  exit 1
fi
```

## Next Steps

- **[Watch Mode](./watch-mode)** - Real-time validation
- **[Commands](./commands)** - CLI reference
- **[Configuration](./configuration)** - Configure validation rules

---

**Having issues?** Check the VSA documentation and examples in the repository for guidance.

