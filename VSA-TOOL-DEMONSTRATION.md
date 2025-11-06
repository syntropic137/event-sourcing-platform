# VSA Tool Demonstration: How It Unifies Everything

**Date:** 2025-11-06  
**Purpose:** Show how the VSA CLI tool provides a unified approach to building and validating event-sourced applications

---

## 🎯 The Problem: Architectural Drift

Without VSA tool:
- ❌ Developers place aggregates in wrong locations
- ❌ Business logic leaks into adapters
- ❌ Event versioning is inconsistent
- ❌ Cross-slice dependencies sneak in
- ❌ ADR compliance is manual and error-prone
- ❌ No automated validation
- ❌ Architecture degrades over time

With VSA tool:
- ✅ Single source of truth (`vsa.yaml`)
- ✅ Automated ADR compliance checking
- ✅ Fail-fast feedback during development
- ✅ Consistent patterns across all projects
- ✅ Self-documenting architecture
- ✅ CI/CD integration

---

## 🔧 Core Commands

### 1. `vsa validate` - Architecture Validation

**Purpose:** Verify project follows ADR-compliant hexagonal VSA architecture

**Usage:**
```bash
cd examples/002-simple-aggregate-ts
vsa validate
```

**Example Output (SUCCESS):**
```
🔍 Validating Hexagonal Event-Sourced VSA Architecture
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

📦 Scanning Project Structure...
  ✅ Found vsa.yaml (version: 2)
  ✅ Architecture: hexagonal-event-sourced-vsa
  ✅ Language: typescript
  ✅ Root: ./src

🏛️  Domain Layer Analysis
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  ✅ Domain folder exists: src/domain
  
  Aggregates:
  ✅ OrderAggregate (src/domain/OrderAggregate.ts)
     - Command handlers: 2 (@SubmitOrderCommand, @CancelOrderCommand)
     - Event handlers: 2 (@OrderSubmitted, @OrderCancelled)
     - Lines: 115
  
  Commands:
  ✅ SubmitOrderCommand (src/domain/commands/SubmitOrderCommand.ts)
     - Has aggregateId: ✅
     - Fields: orderId, customerId
  ✅ CancelOrderCommand (src/domain/commands/CancelOrderCommand.ts)
     - Has aggregateId: ✅
     - Fields: reason
  
  Events:
  ✅ OrderSubmittedEvent (src/domain/events/OrderSubmittedEvent.ts)
     - Version: v1 (schemaVersion: 1)
     - Fields: orderId, customerId
  ✅ OrderCancelledEvent (src/domain/events/OrderCancelledEvent.ts)
     - Version: v1 (schemaVersion: 1)
     - Fields: reason

🔌 Adapter Layer Analysis (Slices)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  ✅ Slices folder exists: src/slices
  
  Command Slices:
  ✅ submit-order (src/slices/submit-order/SubmitOrderCli.ts)
     - Lines: 15 (< 50 ✅)
     - Uses CommandBus: ✅
     - Business logic detected: ❌ (good!)
  ✅ cancel-order (src/slices/cancel-order/CancelOrderCli.ts)
     - Lines: 15 (< 50 ✅)
     - Uses CommandBus: ✅
     - Business logic detected: ❌ (good!)

🏗️  Infrastructure Layer Analysis
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  ✅ Infrastructure folder exists: src/infrastructure
  ✅ CommandBus.ts found

🔐 Dependency Analysis
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  ✅ Domain layer has no outward dependencies
  ✅ No cross-slice imports detected
  ✅ Dependency direction correct: Slices → Infrastructure → Domain

📋 Validation Summary
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  ✅ HEX001: Domain isolation maintained
  ✅ HEX002: No cross-slice dependencies
  ✅ HEX003: Slices are thin adapters
  ✅ DOM001: Aggregates in domain/ folder
  ✅ DOM002: Commands have aggregateId
  ✅ EVT001: Events have versions
  ✅ SLICE001: Slices under 50 lines
  ✅ CQRS001: Commands use CommandBus

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
✅ All checks passed! Architecture is compliant.
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Scanned:
  - 1 aggregate
  - 2 commands
  - 2 events
  - 2 slices
  - 0 violations

Time: 0.8s
```

**Example Output (FAILURE):**
```
🔍 Validating Hexagonal Event-Sourced VSA Architecture
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

📦 Scanning Project Structure...
  ✅ Found vsa.yaml (version: 2)
  
❌ Validation Failed!
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

[HEX003] Business logic detected in slice
  File: src/slices/submit-order/SubmitOrderCli.ts:18
  Issue: Validation logic found (title.length > 100)
  Suggestion: Move validation to OrderAggregate.submit() method
  
[SLICE001] Slice exceeds maximum lines
  File: src/slices/submit-order/SubmitOrderCli.ts
  Lines: 85
  Maximum: 50
  Suggestion: Extract logic to infrastructure or domain layer

[DOM001] Aggregate in wrong location
  File: src/slices/submit-order/OrderAggregate.ts
  Expected: src/domain/OrderAggregate.ts
  Suggestion: Move aggregate to domain/ folder (shared across slices)

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
❌ 3 violations found. Fix these issues to comply with ADRs.
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Exit code: 1
```

---

### 2. `vsa scan` - Visualize Architecture

**Purpose:** Scan project and generate architecture diagrams

**Usage:**
```bash
vsa scan                    # Interactive terminal output
vsa scan --format json      # Machine-readable output
vsa scan --output diagram.svg  # Generate diagram
```

**Example Output:**
```
📊 Architecture Scan: 002-simple-aggregate-ts
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

🏛️  Domain Layer (Hexagon Center)
├── OrderAggregate
│   ├── @CommandHandler: SubmitOrderCommand → OrderSubmittedEvent
│   └── @CommandHandler: CancelOrderCommand → OrderCancelledEvent
├── Commands
│   ├── SubmitOrderCommand (aggregateId ✅)
│   └── CancelOrderCommand (aggregateId ✅)
└── Events
    ├── OrderSubmittedEvent (v1)
    └── OrderCancelledEvent (v1)

🏗️  Infrastructure Layer (Application Services)
└── CommandBus
    ├── Routes: SubmitOrderCommand → OrderAggregate.submit()
    └── Routes: CancelOrderCommand → OrderAggregate.cancel()

🔌 Adapter Layer (Hexagon Outside)
├── submit-order/ (Command Slice)
│   └── SubmitOrderCli → CommandBus.send(SubmitOrderCommand)
└── cancel-order/ (Command Slice)
    └── CancelOrderCli → CommandBus.send(CancelOrderCommand)

Dependency Flow:
  CLI Adapter → CommandBus → OrderAggregate → Events → EventStore
```

---

### 3. `vsa generate` - Scaffold Components

**Purpose:** Generate new architecture components following ADR patterns

**Generate Command Slice:**
```bash
vsa generate slice update-order --type command
```

**Output:**
```
🎨 Generating Command Slice: update-order
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Created files:
  ✅ src/domain/commands/UpdateOrderCommand.ts
  ✅ src/domain/events/OrderUpdatedEvent.ts  
  ✅ src/slices/update-order/UpdateOrderCli.ts
  ✅ src/slices/update-order/UpdateOrderCli.test.ts

Updated files:
  ✅ src/domain/OrderAggregate.ts
     - Added @CommandHandler for UpdateOrderCommand
     - Added @EventSourcingHandler for OrderUpdatedEvent

Next steps:
  1. Implement business logic in OrderAggregate.update()
  2. Run: npm test
  3. Run: vsa validate

Hint: Keep UpdateOrderCli.ts under 50 lines (thin adapter!)
```

**Generate Query Slice:**
```bash
vsa generate slice get-order --type query
```

**Output:**
```
🎨 Generating Query Slice: get-order
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Created files:
  ✅ src/domain/queries/GetOrderQuery.ts
  ✅ src/slices/get-order/GetOrderCli.ts
  ✅ src/slices/get-order/OrderProjection.ts
  ✅ src/slices/get-order/OrderProjection.test.ts

Updated files:
  ✅ src/infrastructure/QueryBus.ts (created if missing)

Next steps:
  1. Implement OrderProjection to build read model from events
  2. Implement GetOrderCli to query the projection
  3. Run: vsa validate
```

---

### 4. `vsa doctor` - Diagnose Issues

**Purpose:** Analyze project health and suggest improvements

**Usage:**
```bash
vsa doctor
```

**Example Output:**
```
🩺 VSA Health Check: 002-simple-aggregate-ts
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

✅ EXCELLENT (95/100)

Architecture Health:
  ✅ Domain isolation: Perfect
  ✅ Slice separation: Perfect
  ✅ Event versioning: Good
  ⚠️  Test coverage: 75% (target: 90%)
  ⚠️  Documentation: Missing slice.yaml files

Recommendations:
  1. Add slice.yaml metadata to slices/
  2. Increase test coverage for edge cases
  3. Consider adding integration tests
  4. Document business rules in aggregate

Dependencies:
  ✅ No circular dependencies
  ✅ All dependencies point inward
  
Performance:
  ✅ Aggregate complexity: Low (2 command handlers)
  ✅ Event size: Optimal (< 1KB)
  
Security:
  ⚠️  Consider adding command authorization
  ℹ️  Aggregate IDs should be validated
```

---

## 📋 vsa.yaml: The Unifying Configuration

**Single source of truth for architecture:**

```yaml
# vsa.yaml - Architecture configuration
version: 2
architecture: "hexagonal-event-sourced-vsa"
language: "typescript"
root: "./src"

domain:
  path: "domain"
  aggregates:
    pattern: "*Aggregate.ts"
    require_suffix: true
  commands:
    path: "commands"
    pattern: "*Command.ts"
    require_aggregate_id: true
  events:
    path: "events"
    pattern: "*Event.ts"
    versioning:
      enabled: true
      format: "simple"  # v1, v2, v3

slices:
  path: "slices"
  command_slices:
    pattern: "**/*Cli.ts"
    max_lines: 50  # Enforce thin adapters!
    require_command_bus: true
  query_slices:
    pattern: "**/*Query.ts"
    require_projection: true

infrastructure:
  path: "infrastructure"

validation:
  architecture:
    enforce_hexagonal: true
    enforce_dependency_direction: true
  slices:
    enforce_isolation: true  # No cross-slice imports
    enforce_thin_adapters: true
```

**Benefits:**
1. ✅ **Consistency** - Same config across all projects
2. ✅ **Validation** - Automated compliance checking
3. ✅ **Documentation** - Self-documenting architecture
4. ✅ **Tooling** - IDE plugins, CI/CD integration
5. ✅ **Evolvability** - Easy to update architecture rules

---

## 🔄 Development Workflow with VSA

### Initial Setup
```bash
# 1. Clone project
git clone <repo>
cd examples/004-cqrs-patterns-ts

# 2. Install dependencies
npm install

# 3. Validate architecture
npm run validate
# or: vsa validate
```

### Adding New Feature
```bash
# 1. Generate new command slice
vsa generate slice withdraw-money --type command

# 2. Implement business logic in aggregate
vim src/domain/AccountAggregate.ts

# 3. Validate architecture
vsa validate

# 4. Run tests
npm test

# 5. Commit (architecture validated in pre-commit hook)
git commit -m "feat: add withdraw money command"
```

### Refactoring
```bash
# 1. Check current state
vsa scan

# 2. Make changes
# ... edit files ...

# 3. Validate after each change
vsa validate

# 4. See what changed
vsa scan --format json > after.json
diff before.json after.json
```

### CI/CD Integration
```yaml
# .github/workflows/validate.yml
name: Validate Architecture

on: [push, pull_request]

jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Setup VSA CLI
        run: cargo install vsa-cli
      
      - name: Validate Architecture
        run: |
          for example in examples/*/; do
            cd "$example"
            vsa validate || exit 1
            cd -
          done
```

---

## 🌟 Real-World Example: 004-cqrs-patterns-ts

**Before VSA Tool:**
```
❌ Aggregates scattered in slice folders
❌ Business logic in CLI adapters
❌ No validation of architecture
❌ Manual compliance checking
❌ Inconsistent patterns
```

**After VSA Tool:**
```
✅ Run: vsa validate
   → Finds: AccountAggregate in domain/
   → Validates: Commands have aggregateId
   → Checks: Events have versions
   → Verifies: Slices are < 50 lines
   → Ensures: No cross-slice dependencies
   
✅ Run: vsa scan
   → Shows: Complete architecture visualization
   → Lists: All aggregates, commands, events, slices
   → Validates: Dependency flow correctness
   
✅ Run: vsa generate slice close-account --type command
   → Creates: CloseAccountCommand.ts
   → Creates: AccountClosedEvent.ts
   → Creates: CloseAccountCli.ts
   → Updates: AccountAggregate.ts with handler
   → Generates: Tests
```

---

## 🎯 Key Takeaways

### Without VSA Tool:
- Manual architecture validation
- Inconsistent patterns
- Architectural drift
- No automated enforcement
- Hard to onboard new developers
- Documentation gets outdated

### With VSA Tool:
- ✅ **Automated** - `vsa validate` catches issues
- ✅ **Consistent** - Same patterns everywhere
- ✅ **Fast** - Fail-fast feedback
- ✅ **Documented** - vsa.yaml is always current
- ✅ **Scalable** - Works for any project size
- ✅ **Cross-Language** - TypeScript, Python, Rust

---

## 🚀 Next Steps

1. **Use VSA in every example** - All examples/ have vsa.yaml
2. **CI/CD Integration** - Run `vsa validate` in pipelines
3. **IDE Integration** - Real-time validation in VS Code
4. **Team Adoption** - Make VSA validation required
5. **Expand Validation** - Add more ADR rules

**The VSA tool is the glue that makes Hexagonal Event-Sourced VSA Architecture practical and maintainable at scale.**

---

**Status:** Documentation complete, implementation in progress (vsa-core Milestones 3-7)

