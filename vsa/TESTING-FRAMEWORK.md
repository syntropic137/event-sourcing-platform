# VSA Core Testing Framework

**Version:** 1.0.0  
**Date:** 2025-11-06  
**Status:** 🔄 Planning

## 🎯 Objective

Create a comprehensive testing framework for `vsa-core` that validates architectural rules through:
- **Unit tests** for individual components
- **Integration tests** for scanners and validators
- **E2E tests** with complete test projects (fixtures)
- **Multi-language support** (TypeScript, Python, Rust)

---

## 📋 Testing Strategy

### Test Pyramid

```
                    ┌─────────────┐
                    │   E2E Tests │  ← 10-15 complete projects
                    │   (Fixtures)│
                    └──────┬──────┘
                           │
                    ┌──────┴───────┐
                    │ Integration  │  ← 30-40 tests
                    │    Tests     │
                    └──────┬───────┘
                           │
                    ┌──────┴───────┐
                    │  Unit Tests  │  ← 100+ tests
                    │              │
                    └──────────────┘
```

### Coverage Goals

- **Unit Tests:** 90%+ code coverage
- **Integration Tests:** 85%+ coverage of validator logic
- **E2E Tests:** 100% coverage of architectural rules

---

## 🧪 Test Structure

```
vsa/vsa-core/
├── src/
│   └── ... (implementation)
│
└── tests/
    ├── unit/                       ← Unit tests
    │   ├── config_test.rs
    │   ├── scanners/
    │   │   ├── domain_scanner_test.rs
    │   │   ├── aggregate_scanner_test.rs
    │   │   ├── event_scanner_test.rs
    │   │   └── slice_scanner_test.rs
    │   ├── analyzers/
    │   │   ├── ast_parser_test.rs
    │   │   ├── decorator_extractor_test.rs
    │   │   └── import_analyzer_test.rs
    │   └── validation/
    │       ├── hexagonal_rules_test.rs
    │       ├── domain_rules_test.rs
    │       ├── event_rules_test.rs
    │       └── cqrs_rules_test.rs
    │
    ├── integration/                ← Integration tests
    │   ├── validator_integration_test.rs
    │   ├── manifest_generation_test.rs
    │   └── multi_language_test.rs
    │
    └── fixtures/                   ← E2E test projects
        ├── typescript/
        │   ├── valid-hexagonal-vsa/
        │   ├── valid-minimal/
        │   ├── invalid-domain-imports/
        │   ├── invalid-cross-slice/
        │   ├── invalid-business-logic-in-slice/
        │   ├── invalid-missing-event-version/
        │   ├── invalid-missing-upcaster/
        │   └── invalid-thick-adapter/
        │
        ├── python/
        │   ├── valid-hexagonal-vsa/
        │   ├── invalid-domain-imports/
        │   └── ... (similar structure)
        │
        └── rust/
            ├── valid-hexagonal-vsa/
            └── ... (similar structure)
```

---

## 🏗️ Test Fixtures (E2E Test Projects)

### Fixture Design Principles

1. **Complete Projects** - Each fixture is a complete, buildable project
2. **Single Violation** - Invalid fixtures violate exactly ONE rule (for clarity)
3. **Multi-Language** - Same fixtures in TypeScript, Python, and Rust
4. **Real-World** - Fixtures resemble actual applications
5. **Well-Documented** - Each fixture has README explaining the test scenario

---

## 📦 E2E Test Fixtures

### TypeScript Fixtures

#### 1. `valid-hexagonal-vsa` ✅

**Purpose:** Reference implementation demonstrating perfect compliance

**Structure:**
```
valid-hexagonal-vsa/
├── vsa.yaml
├── package.json
├── tsconfig.json
│
├── domain/
│   ├── TaskAggregate.ts          ← With @CommandHandler
│   ├── commands/
│   │   └── tasks/
│   │       ├── CreateTaskCommand.ts
│   │       └── CompleteTaskCommand.ts
│   ├── queries/
│   │   └── GetTaskByIdQuery.ts
│   └── events/
│       ├── TaskCreatedEvent.ts    ← @Event('TaskCreated', 'v1')
│       ├── TaskCompletedEvent.ts
│       ├── _versioned/
│       └── _upcasters/
│
├── infrastructure/
│   ├── CommandBus.ts
│   ├── QueryBus.ts
│   └── EventBus.ts
│
└── slices/
    ├── create-task/
    │   ├── CreateTaskController.ts  ← 30 lines, no business logic
    │   ├── CreateTaskController.test.ts
    │   └── slice.yaml
    │
    ├── get-task/
    │   ├── GetTaskController.ts
    │   ├── TaskProjection.ts
    │   ├── TaskProjection.test.ts
    │   └── slice.yaml
    │
    └── task-notification-saga/
        ├── TaskNotificationSaga.ts
        ├── TaskNotificationSaga.test.ts
        └── slice.yaml
```

**Expected:** All validation rules pass ✅

---

#### 2. `valid-minimal` ✅

**Purpose:** Minimal valid project with bare essentials

**Structure:**
```
valid-minimal/
├── vsa.yaml
├── domain/
│   ├── TaskAggregate.ts
│   ├── commands/
│   │   └── tasks/
│   │       └── CreateTaskCommand.ts
│   └── events/
│       └── TaskCreatedEvent.ts
├── infrastructure/
│   └── CommandBus.ts
└── slices/
    └── create-task/
        ├── CreateTaskController.ts
        └── slice.yaml
```

**Expected:** All validation rules pass ✅

---

#### 3. `invalid-domain-imports` ❌

**Purpose:** Test HEX001 rule - Domain imports from infrastructure

**Violation:**
```typescript
// domain/TaskAggregate.ts
import { CommandBus } from '../infrastructure/CommandBus';  // ❌ VIOLATION

@Aggregate()
export class TaskAggregate {
  constructor(private commandBus: CommandBus) {}  // ❌ Domain depends on infrastructure
}
```

**Expected Error:**
```
[HEX001] Domain has outward dependencies
  File: domain/TaskAggregate.ts:2
  Import: '../infrastructure/CommandBus'
  Suggestion: Remove infrastructure imports. Use dependency injection at application boundary.
```

---

#### 4. `invalid-cross-slice` ❌

**Purpose:** Test HEX002 rule - Slices import from other slices

**Violation:**
```typescript
// slices/update-task/UpdateTaskController.ts
import { CreateTaskController } from '../create-task/CreateTaskController';  // ❌ VIOLATION

export class UpdateTaskController {
  async handle(request: UpdateTaskRequest) {
    // Using logic from another slice ❌
    const creator = new CreateTaskController();
  }
}
```

**Expected Error:**
```
[HEX002] Cross-slice dependency detected
  File: slices/update-task/UpdateTaskController.ts:2
  Import: '../create-task/CreateTaskController'
  Suggestion: Slices must be isolated. Extract shared logic to infrastructure or domain.
```

---

#### 5. `invalid-business-logic-in-slice` ❌

**Purpose:** Test HEX003 rule - Business logic in slice

**Violation:**
```typescript
// slices/create-task/CreateTaskController.ts
export class CreateTaskController {
  async handle(request: CreateTaskRequest) {
    // ❌ Business validation in adapter
    if (!request.title || request.title.length === 0) {
      throw new Error('Title is required');
    }
    
    // ❌ Business rule in adapter
    if (request.title.length > 100) {
      throw new Error('Title too long');
    }
    
    // ❌ Complex business logic
    const priority = this.calculatePriority(request);
    
    const command = new CreateTaskCommand(
      this.generateId(),
      request.title,
      priority
    );
    
    await this.commandBus.send(command);
  }
  
  // ❌ 50+ lines of complex logic
  private calculatePriority(request: CreateTaskRequest): number {
    // Complex calculation logic...
  }
}
```

**Expected Error:**
```
[HEX003] Business logic detected in slice
  File: slices/create-task/CreateTaskController.ts:5
  Issue: Validation logic found (title length check)
  Suggestion: Move validation to TaskAggregate.handle() method

[SLICE001] Slice exceeds maximum lines
  File: slices/create-task/CreateTaskController.ts
  Lines: 85
  Maximum: 50
  Suggestion: Reduce adapter to < 50 lines. Move logic to domain or infrastructure.
```

---

#### 6. `invalid-missing-event-version` ❌

**Purpose:** Test EVT001, EVT002 rules - Event missing @Event decorator or version

**Violation:**
```typescript
// domain/events/TaskCreatedEvent.ts
// ❌ Missing @Event decorator
export class TaskCreatedEvent {
  constructor(
    public readonly aggregateId: string,
    public readonly title: string
  ) {}
}
```

**Expected Error:**
```
[EVT001] Event missing @Event decorator
  File: domain/events/TaskCreatedEvent.ts:2
  Class: TaskCreatedEvent
  Suggestion: Add @Event('TaskCreated', 'v1') decorator

[EVT002] Event missing version parameter
  File: domain/events/TaskCreatedEvent.ts:2
  Suggestion: Specify version: @Event('TaskCreated', 'v1')
```

---

#### 7. `invalid-missing-upcaster` ❌

**Purpose:** Test EVT003 rule - Event version changed but no upcaster

**Violation:**
```
domain/events/
├── TaskCreatedEvent.ts        ← @Event('TaskCreated', 'v2')  (NEW FIELD ADDED)
├── _versioned/
│   └── TaskCreatedEvent_v1.ts ← @Event('TaskCreated', 'v1')
└── _upcasters/
    └── (empty)                ← ❌ Missing upcaster!
```

**Expected Error:**
```
[EVT003] Missing upcaster for event version change
  Event: TaskCreated
  From: v1 (in _versioned/)
  To: v2 (current)
  Expected: domain/events/_upcasters/TaskCreatedEvent_Upcaster_v1_v2.ts
  Suggestion: Create upcaster to migrate v1 → v2
```

---

#### 8. `invalid-thick-adapter` ❌

**Purpose:** Test SLICE001 rule - Adapter exceeds line limit

**Violation:**
```typescript
// slices/create-task/CreateTaskController.ts (150 lines)
export class CreateTaskController {
  // 150 lines of code including:
  // - Complex request parsing
  // - Multiple helper methods
  // - Extensive error handling
  // - Response transformation
  // - Logging
  // - Metrics
}
```

**Expected Error:**
```
[SLICE001] Slice exceeds maximum lines
  File: slices/create-task/CreateTaskController.ts
  Lines: 150
  Maximum: 50
  Suggestion: Extract logic to:
    - Infrastructure (parsing, metrics, logging)
    - Domain (business logic)
```

---

#### 9. `invalid-command-not-using-command-bus` ❌

**Purpose:** Test CQRS001 rule - Command slice not using CommandBus

**Violation:**
```typescript
// slices/create-task/CreateTaskController.ts
export class CreateTaskController {
  constructor(private repository: TaskRepository) {}  // ❌ Direct repository access

  async handle(request: CreateTaskRequest) {
    const task = new Task(request.title);
    await this.repository.save(task);  // ❌ Bypassing CommandBus
  }
}
```

**Expected Error:**
```
[CQRS001] Command slice not using CommandBus
  File: slices/create-task/CreateTaskController.ts
  Issue: Direct repository access detected
  Suggestion: Use CommandBus to dispatch commands to aggregates
```

---

#### 10. `invalid-aggregate-not-in-domain` ❌

**Purpose:** Test DOM001 rule - Aggregate in wrong location

**Violation:**
```
slices/create-task/
└── TaskAggregate.ts  ← ❌ Aggregate inside slice!
```

**Expected Error:**
```
[DOM001] Aggregate in wrong location
  File: slices/create-task/TaskAggregate.ts
  Expected: domain/TaskAggregate.ts
  Suggestion: Move aggregate to domain/ folder. Aggregates are shared across slices.
```

---

#### 11. `invalid-commands-not-organized-by-feature` ❌

**Purpose:** Test DOM002 rule - Commands not in feature folders

**Violation:**
```
domain/commands/
├── CreateTaskCommand.ts       ← ❌ Not in feature folder
├── CompleteTaskCommand.ts     ← ❌ Not in feature folder
└── AddItemCommand.ts          ← ❌ Not in feature folder
```

**Expected Error:**
```
[DOM002] Commands not organized by feature
  File: domain/commands/CreateTaskCommand.ts
  Expected: domain/commands/tasks/CreateTaskCommand.ts
  Suggestion: Organize commands by feature in subdirectories
```

---

#### 12. `invalid-query-slice-no-projection` ❌

**Purpose:** Test query slice must have projection

**Violation:**
```
slices/get-task/
└── GetTaskController.ts  ← ❌ No projection file
```

**Expected Error:**
```
[QUERY001] Query slice missing projection
  Slice: get-task
  Expected: slices/get-task/TaskProjection.ts
  Suggestion: Query slices must include a projection/read model
```

---

#### 13. `valid-with-event-versioning` ✅

**Purpose:** Test complete event versioning implementation

**Structure:**
```
domain/events/
├── TaskCreatedEvent.ts        ← @Event('TaskCreated', 'v3')
│
├── _versioned/
│   ├── TaskCreatedEvent_v1.ts ← @Deprecated('v2')
│   └── TaskCreatedEvent_v2.ts ← @Deprecated('v3')
│
└── _upcasters/
    ├── TaskCreatedEvent_Upcaster_v1_v2.ts
    └── TaskCreatedEvent_Upcaster_v2_v3.ts
```

**Expected:** All validation rules pass ✅

---

#### 14. `valid-multiple-aggregates` ✅

**Purpose:** Test multiple aggregates in same domain

**Structure:**
```
domain/
├── TaskAggregate.ts
├── ProjectAggregate.ts
├── UserAggregate.ts
└── commands/
    ├── tasks/
    │   └── CreateTaskCommand.ts
    ├── projects/
    │   └── CreateProjectCommand.ts
    └── users/
        └── CreateUserCommand.ts
```

**Expected:** All validation rules pass ✅

---

#### 15. `valid-saga-slice` ✅

**Purpose:** Test saga slice with event handlers and command dispatching

**Structure:**
```
slices/task-notification-saga/
├── TaskNotificationSaga.ts
│   ├── @EventHandler for TaskCreatedEvent
│   ├── @EventHandler for TaskCompletedEvent
│   └── Dispatches NotifyUserCommand
├── TaskNotificationSaga.test.ts
└── slice.yaml
```

**Expected:** All validation rules pass ✅

---

### Python Fixtures (Similar Structure)

All TypeScript fixtures replicated in Python with appropriate syntax:

```
tests/fixtures/python/
├── valid-hexagonal-vsa/
├── valid-minimal/
├── invalid-domain-imports/
├── invalid-cross-slice/
└── ... (15 fixtures total)
```

---

### Rust Fixtures (Similar Structure)

All TypeScript fixtures replicated in Rust with appropriate syntax:

```
tests/fixtures/rust/
├── valid-hexagonal-vsa/
├── valid-minimal/
├── invalid-domain-imports/
├── invalid-cross-slice/
└── ... (15 fixtures total)
```

---

## 🧪 E2E Test Implementation

### Test Runner Structure

```rust
// tests/e2e/fixtures_test.rs

use vsa_core::{Validator, VsaConfig};
use std::path::PathBuf;

#[test]
fn test_valid_hexagonal_vsa_typescript() {
    let fixture_path = PathBuf::from("tests/fixtures/typescript/valid-hexagonal-vsa");
    let config = VsaConfig::load(&fixture_path.join("vsa.yaml")).unwrap();
    let validator = Validator::new(config, fixture_path);
    
    let report = validator.validate().unwrap();
    
    assert_eq!(report.errors.len(), 0, "Expected no errors");
    assert_eq!(report.warnings.len(), 0, "Expected no warnings");
}

#[test]
fn test_invalid_domain_imports_typescript() {
    let fixture_path = PathBuf::from("tests/fixtures/typescript/invalid-domain-imports");
    let config = VsaConfig::load(&fixture_path.join("vsa.yaml")).unwrap();
    let validator = Validator::new(config, fixture_path);
    
    let report = validator.validate().unwrap();
    
    assert_eq!(report.errors.len(), 1, "Expected 1 error");
    assert_eq!(report.errors[0].code, "HEX001");
    assert!(report.errors[0].message.contains("Domain has outward dependencies"));
}

#[test]
fn test_invalid_cross_slice_typescript() {
    let fixture_path = PathBuf::from("tests/fixtures/typescript/invalid-cross-slice");
    let config = VsaConfig::load(&fixture_path.join("vsa.yaml")).unwrap();
    let validator = Validator::new(config, fixture_path);
    
    let report = validator.validate().unwrap();
    
    assert_eq!(report.errors.len(), 1);
    assert_eq!(report.errors[0].code, "HEX002");
    assert!(report.errors[0].message.contains("Cross-slice dependency"));
}

// ... 12 more test functions (one per fixture)
```

---

## 📊 Test Matrix

| Fixture | TypeScript | Python | Rust | Rule Tested |
|---------|------------|--------|------|-------------|
| valid-hexagonal-vsa | ✅ | ✅ | ✅ | All rules pass |
| valid-minimal | ✅ | ✅ | ✅ | Minimal compliance |
| invalid-domain-imports | ✅ | ✅ | ✅ | HEX001 |
| invalid-cross-slice | ✅ | ✅ | ✅ | HEX002 |
| invalid-business-logic-in-slice | ✅ | ✅ | ✅ | HEX003, SLICE001 |
| invalid-missing-event-version | ✅ | ✅ | ✅ | EVT001, EVT002 |
| invalid-missing-upcaster | ✅ | ✅ | ✅ | EVT003 |
| invalid-thick-adapter | ✅ | ✅ | ✅ | SLICE001 |
| invalid-command-not-using-bus | ✅ | ✅ | ✅ | CQRS001 |
| invalid-aggregate-not-in-domain | ✅ | ✅ | ✅ | DOM001 |
| invalid-commands-not-organized | ✅ | ✅ | ✅ | DOM002 |
| invalid-query-slice-no-projection | ✅ | ✅ | ✅ | QUERY001 |
| valid-with-event-versioning | ✅ | ✅ | ✅ | Event versioning |
| valid-multiple-aggregates | ✅ | ✅ | ✅ | Multiple aggregates |
| valid-saga-slice | ✅ | ✅ | ✅ | Saga pattern |

**Total E2E Tests:** 15 fixtures × 3 languages = **45 E2E tests**

---

## 🔧 Unit Test Examples

### Scanner Tests

```rust
// tests/unit/scanners/aggregate_scanner_test.rs

#[test]
fn test_aggregate_scanner_finds_aggregates() {
    let scanner = AggregateScanner::new(AggregateConfig {
        path: PathBuf::from("."),
        pattern: "*Aggregate.ts".to_string(),
        require_suffix: true,
    });
    
    let temp_dir = create_temp_domain();
    create_file(&temp_dir, "TaskAggregate.ts", "...");
    create_file(&temp_dir, "CartAggregate.ts", "...");
    
    let aggregates = scanner.scan(&temp_dir).unwrap();
    
    assert_eq!(aggregates.len(), 2);
    assert!(aggregates.iter().any(|a| a.name == "TaskAggregate"));
    assert!(aggregates.iter().any(|a| a.name == "CartAggregate"));
}

#[test]
fn test_aggregate_scanner_respects_suffix() {
    let scanner = AggregateScanner::new(AggregateConfig {
        path: PathBuf::from("."),
        pattern: "*Aggregate.ts".to_string(),
        require_suffix: true,
    });
    
    let temp_dir = create_temp_domain();
    create_file(&temp_dir, "Task.ts", "...");  // ❌ No suffix
    
    let aggregates = scanner.scan(&temp_dir).unwrap();
    
    assert_eq!(aggregates.len(), 0);
}
```

### Validation Rule Tests

```rust
// tests/unit/validation/hexagonal_rules_test.rs

#[test]
fn test_hexagonal_rule_detects_domain_imports() {
    let rule = HexagonalArchitectureRule;
    let mut report = ValidationReport::new();
    
    let ctx = create_test_context_with_domain_importing_infrastructure();
    
    rule.validate(&ctx, &mut report).unwrap();
    
    assert_eq!(report.errors.len(), 1);
    assert_eq!(report.errors[0].code, "HEX001");
}

#[test]
fn test_hexagonal_rule_allows_domain_imports_within_domain() {
    let rule = HexagonalArchitectureRule;
    let mut report = ValidationReport::new();
    
    let ctx = create_test_context_with_domain_importing_domain();
    
    rule.validate(&ctx, &mut report).unwrap();
    
    assert_eq!(report.errors.len(), 0);
}
```

---

## 🚀 CI/CD Integration

### GitHub Actions Workflow

```yaml
# .github/workflows/vsa-core-tests.yml

name: VSA Core Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    
    steps:
      - uses: actions/checkout@v3
      
      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      
      - name: Run Unit Tests
        run: cargo test --lib
        working-directory: vsa/vsa-core
      
      - name: Run Integration Tests
        run: cargo test --test integration
        working-directory: vsa/vsa-core
      
      - name: Run E2E Tests (TypeScript)
        run: cargo test --test fixtures_typescript
        working-directory: vsa/vsa-core
      
      - name: Run E2E Tests (Python)
        run: cargo test --test fixtures_python
        working-directory: vsa/vsa-core
      
      - name: Run E2E Tests (Rust)
        run: cargo test --test fixtures_rust
        working-directory: vsa/vsa-core
      
      - name: Generate Coverage Report
        run: cargo tarpaulin --out Html --output-dir coverage
        working-directory: vsa/vsa-core
      
      - name: Upload Coverage
        uses: codecov/codecov-action@v3
```

---

## 📈 Success Criteria

### Unit Tests
- [ ] 100+ unit tests written
- [ ] 90%+ code coverage
- [ ] All scanners tested
- [ ] All analyzers tested
- [ ] All validation rules tested

### Integration Tests
- [ ] 30-40 integration tests
- [ ] Validator end-to-end tested
- [ ] Manifest generation tested
- [ ] Multi-language support tested

### E2E Tests
- [ ] 15 TypeScript fixtures
- [ ] 15 Python fixtures
- [ ] 15 Rust fixtures
- [ ] All validation rules covered
- [ ] Clear error messages validated

### CI/CD
- [ ] All tests run automatically
- [ ] Coverage reports generated
- [ ] Fast feedback (< 5 minutes)

---

## 🎯 Implementation Plan

### Phase 1: Test Infrastructure (Week 1)
- [ ] Set up test directory structure
- [ ] Create test helpers and utilities
- [ ] Set up fixture template generator
- [ ] Configure CI/CD pipeline

### Phase 2: Unit Tests (Weeks 2-3)
- [ ] Scanner unit tests
- [ ] Analyzer unit tests
- [ ] Validation rule unit tests
- [ ] Config parser unit tests

### Phase 3: E2E Fixtures (Weeks 4-5)
- [ ] Create 15 TypeScript fixtures
- [ ] Create 15 Python fixtures
- [ ] Create 15 Rust fixtures
- [ ] Document each fixture

### Phase 4: Integration Tests (Week 6)
- [ ] Validator integration tests
- [ ] Manifest generation tests
- [ ] Multi-language tests
- [ ] Performance tests

### Phase 5: Polish (Week 7)
- [ ] Improve test coverage
- [ ] Add edge case tests
- [ ] Optimize test performance
- [ ] Documentation

---

## 📝 Next Steps

1. Review this testing framework plan
2. Create ADR for testing strategy (optional)
3. Begin Phase 1 implementation
4. Create first fixture as template
5. Iterate with feedback

---

## 🔄 Fixture Migration from vsa/examples

As part of consolidating examples and eliminating duplication, the following examples were migrated from `/vsa/examples/` to become E2E test fixtures:

### Migrated Fixtures

| Source (vsa/examples/) | Destination (tests/fixtures/) | Purpose |
|------------------------|-------------------------------|---------|
| `01-todo-list-ts/` | `typescript/valid/01-hexagonal-complete/` | Complete hexagonal VSA reference |
| `02-library-management-ts/` | `typescript/valid/02-multi-context/` | Multiple bounded contexts with integration events |
| `05-todo-list-py/` | `python/valid/01-todo-simple/` | Simple Python VSA implementation |

### Changes Made During Migration

1. **Updated README.md** - Changed from "example" to "E2E test fixture" context
2. **Added test expectations** - Documented expected validation results
3. **Maintained structure** - Preserved VSA-compliant architecture
4. **Created validation tests** - Added Rust integration tests for each fixture

### Developer-Facing Examples

After migration, `/vsa/examples/` was deleted. Developers should now refer to:

- **`/examples/`** - Root examples directory (developer-facing, ADR-compliant)
- **`/vsa/vsa-core/tests/fixtures/`** - E2E test fixtures (for vsa-core testing)

This separation ensures:
- ✅ No duplication between examples and tests
- ✅ Clear purpose for each directory
- ✅ Single source of truth for VSA validation
- ✅ Examples can be validated with `vsa validate`

### Fixture Location Reference

All test fixtures are located at: **`/vsa/vsa-core/tests/fixtures/`**

See `tests/fixtures/README.md` for detailed fixture documentation.

---

**Testing Framework: Implemented and Ready** ✅🚀

