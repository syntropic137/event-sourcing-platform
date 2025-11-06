# Test Fixtures Migration Status

**Last Updated:** 2025-11-06  
**Project Plan:** `/PROJECT-PLAN_20251106_examples-migration.md`

## 📊 Overview

Migration of `/vsa/examples/` to `/vsa/vsa-core/tests/fixtures/` for E2E testing.

## ✅ Completed

### Milestone 2: Setup Test Fixtures Infrastructure ✅
- [x] Created `/vsa/vsa-core/tests/fixtures/` directory structure
- [x] Created `fixtures/README.md` documentation
- [x] Updated `TESTING-FRAMEWORK.md` with fixture guidelines
- [x] Created `tests/integration/fixture_validation.rs` test file
- [x] Created `tests/integration.rs` test entry point
- [x] Verified directory structure with tests

**Status:** ✅ **COMPLETE**

---

## 🚧 In Progress

### Milestone 3: Migrate VSA Examples to Test Fixtures

#### Phase 3A: Migrate TypeScript Examples

**Current Issue:** The existing `/vsa/examples/` are NOT yet aligned with ADR-006 (Hexagonal Architecture).

**Existing Structure (Old VSA):**
```
vsa/examples/01-todo-list-ts/
└── src/contexts/tasks/
    ├── create-task/
    │   ├── TaskAggregate.ts        ← Aggregate in slice (WRONG per ADR-006)
    │   ├── CreateTaskCommand.ts
    │   └── TaskCreatedEvent.ts
    ├── complete-task/
    └── delete-task/
```

**Required Structure (ADR-006 Compliant):**
```
tests/fixtures/typescript/valid/01-hexagonal-complete/
└── src/contexts/tasks/
    ├── domain/                      ← NEW: Shared domain layer
    │   ├── TaskAggregate.ts         ← Aggregates at root
    │   ├── commands/
    │   │   └── tasks/
    │   │       ├── CreateTaskCommand.ts
    │   │       ├── CompleteTaskCommand.ts
    │   │       └── DeleteTaskCommand.ts
    │   └── events/
    │       ├── TaskCreatedEvent.ts
    │       ├── TaskCompletedEvent.ts
    │       └── TaskDeletedEvent.ts
    └── slices/                      ← Thin adapters only
        ├── create-task/
        ├── complete-task/
        └── delete-task/
```

**Decision:** The `/vsa/examples/` need to be refactored BEFORE migration, not just copied.

---

## 📋 Remaining Work

### Option A: Refactor vsa/examples First, Then Migrate
1. Refactor `/vsa/examples/01-todo-list-ts/` to ADR-006 structure
2. Refactor `/vsa/examples/02-library-management-ts/` to ADR-006 structure  
3. Refactor `/vsa/examples/05-todo-list-py/` to ADR-006 structure
4. Copy refactored examples to `tests/fixtures/`
5. Create invalid fixtures

**Pros:** Examples and fixtures both benefit from ADR-006 structure  
**Cons:** More work, requires refactoring existing examples

### Option B: Create New ADR-Compliant Fixtures from Scratch
1. Delete copied `01-hexagonal-complete/` (not compliant)
2. Create new `01-hexagonal-complete/` from scratch per ADR-006
3. Create new `02-multi-context/` from scratch
4. Create new `01-todo-simple/` (Python) from scratch
5. Create invalid fixtures

**Pros:** Clean slate, no legacy code  
**Cons:** More work, lose existing working code

### Option C (Recommended): Focus on Root `/examples/` First
1. **Skip migrating `/vsa/examples/` for now** (they're outdated anyway)
2. **Focus on Milestone 4**: Refactor root `/examples/` to be ADR-compliant
3. **Create test fixtures** based on refactored root examples
4. **Delete `/vsa/examples/`** after Milestone 4

**Pros:** Single source of truth, less duplication, root examples are more important  
**Cons:** Delay in having test fixtures

---

## 🎯 Recommended Path Forward

### Phase 1: Complete Milestone 4 First
Refactor key root examples to ADR-006 structure:
- `/examples/002-simple-aggregate-ts/` → ADR-compliant
- `/examples/004-cqrs-patterns-ts/` → ADR-compliant
- `/examples/007-ecommerce-complete-ts/` → ADR-compliant

### Phase 2: Create Test Fixtures Based on Refactored Examples
Copy refactored examples to test fixtures:
- `examples/002-simple-aggregate-ts/` → `tests/fixtures/typescript/valid/01-simple/`
- `examples/004-cqrs-patterns-ts/` → `tests/fixtures/typescript/valid/02-cqrs/`
- `examples/007-ecommerce-complete-ts/` → `tests/fixtures/typescript/valid/03-complete/`

### Phase 3: Create Invalid Fixtures
Based on common mistakes:
- `invalid/01-no-domain-folder/`
- `invalid/02-missing-aggregates/`
- `invalid/03-no-event-decorators/`
- `invalid/04-versioned-no-upcaster/`

### Phase 4: Delete `/vsa/examples/`
Once test fixtures exist and root examples are refactored:
- Delete entire `/vsa/examples/` directory
- Update all documentation references

---

## 📝 Current Status Summary

| Milestone | Status | Notes |
|-----------|--------|-------|
| M2: Setup Fixtures Infrastructure | ✅ Complete | Directory structure and tests created |
| M3: Migrate VSA Examples | ⚠️ Blocked | Need to refactor examples first |
| M4: Refactor Root Examples | 📋 Planned | Should do this BEFORE M3 |
| M5: Deprecate Remaining | 📋 Planned | After M4 |
| M6: Delete vsa/examples | 📋 Planned | After M3, M4, M5 |

**Recommendation:** Proceed with Milestone 4 (refactor root examples) before completing Milestone 3.

---

## 🔄 Revised Execution Order

1. ✅ **M2**: Setup test fixtures infrastructure (DONE)
2. 🔄 **M4**: Refactor `/examples/002, 004, 007` to ADR-006 structure (NEXT)
3. 🔄 **M3**: Create test fixtures from refactored examples
4. 🔄 **M5**: Deprecate/archive remaining examples
5. 🔄 **M6**: Delete `/vsa/examples/`

This ensures a single source of truth and avoids migrating outdated structures.

---

## 📞 Next Actions

1. Review this migration status with team
2. Approve revised execution order
3. Proceed with M4 (refactor root examples)
4. Return to M3 after M4 is complete

**Estimated Effort for M4:** 8-12 hours (refactoring 3 examples)

