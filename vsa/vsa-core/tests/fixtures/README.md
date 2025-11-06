# VSA Core Test Fixtures

This directory contains E2E test projects used to validate `vsa-core` functionality.

## Purpose

Test fixtures serve as complete, realistic projects that `vsa-core` scans and validates. They ensure the VSA CLI works correctly across different scenarios, architectures, and languages.

## Structure

```
fixtures/
├── typescript/
│   ├── valid/                  ← Projects that should PASS validation
│   │   ├── 01-hexagonal-complete/     Complete hexagonal VSA example
│   │   └── 02-multi-context/          Multiple bounded contexts
│   └── invalid/                ← Projects that should FAIL validation
│       ├── 01-no-domain-folder/       Missing domain/ directory
│       ├── 02-missing-aggregates/     Commands without aggregates
│       ├── 03-no-event-decorators/    Events missing @Event decorator
│       └── 04-versioned-no-upcaster/  v2 event without v1→v2 upcaster
│
├── python/
│   ├── valid/
│   │   └── 01-todo-simple/            Simple Python VSA example
│   └── invalid/
│
└── rust/
    └── valid/
```

## Fixture Requirements

### Valid Fixtures
- **MUST** have a `vsa.yaml` configuration file
- **MUST** follow the architecture specified in `vsa.yaml` (hexagonal, VSA, etc.)
- **MUST** pass all `vsa validate` checks
- **SHOULD** include a README explaining what they demonstrate
- **SHOULD** be minimal (only what's needed to test the feature)

### Invalid Fixtures
- **MUST** have a `vsa.yaml` configuration file
- **MUST** intentionally violate specific validation rules
- **MUST** include a README explaining WHAT rule they violate and WHY
- **SHOULD** only violate ONE rule (for clear test failures)

## Naming Convention

### Valid Fixtures
- `{number}-{descriptive-name}/`
- Examples: `01-hexagonal-complete/`, `02-multi-context/`, `03-event-versioning/`

### Invalid Fixtures
- `{number}-{violation-description}/`
- Examples: `01-no-domain-folder/`, `02-missing-aggregates/`, `03-invalid-event-version/`

## Creating New Fixtures

### Manual Creation
1. Create directory in appropriate location (`valid/` or `invalid/`)
2. Add `vsa.yaml` configuration
3. Create minimal source structure
4. Add `README.md` explaining the fixture
5. Test with `vsa validate` (from fixture directory)
6. Add validation test in `tests/integration/fixture_validation.rs`

### Using Template Script
```bash
# From vsa/vsa-core directory
../scripts/create-test-fixture.sh typescript valid 03-event-versioning
```

## Testing Fixtures

Fixtures are tested in two ways:

### 1. Direct VSA CLI Testing
```bash
cd fixtures/typescript/valid/01-hexagonal-complete
vsa validate
# Expected: Exit code 0, all checks pass
```

### 2. Rust Integration Tests
```rust
// tests/integration/fixture_validation.rs
#[test]
fn test_valid_hexagonal_complete() {
    let fixture_path = Path::new("tests/fixtures/typescript/valid/01-hexagonal-complete");
    let config = VsaConfig::load(fixture_path.join("vsa.yaml")).unwrap();
    
    let scanner = DomainScanner::new(config.domain.unwrap(), fixture_path.to_path_buf());
    let model = scanner.scan().unwrap();
    
    // Assertions
    assert!(!model.aggregates.is_empty(), "Should find aggregates");
    assert!(!model.commands.is_empty(), "Should find commands");
    assert!(!model.events.is_empty(), "Should find events");
}
```

## Best Practices

### ✅ DO
- Keep fixtures minimal (test ONE thing well)
- Use realistic names (TaskAggregate, CreateTaskCommand, not FooAggregate)
- Include clear comments explaining non-obvious patterns
- Update README when adding new fixtures
- Test both success and failure paths

### ❌ DON'T
- Create massive fixtures (split into multiple fixtures)
- Use production code (keep fixtures simple and focused)
- Skip documentation (every fixture needs a README)
- Test multiple violations in one invalid fixture
- Ignore failing fixture tests

## Fixture Catalog

### TypeScript Valid Fixtures

| Fixture | Purpose | ADRs Demonstrated |
|---------|---------|-------------------|
| `01-hexagonal-complete` | Complete hexagonal VSA with single context | ADR-006, ADR-007, ADR-008 |
| `02-multi-context` | Multiple bounded contexts with integration events | ADR-006, ADR-008, ADR-009 |

### TypeScript Invalid Fixtures

| Fixture | Violation | Expected Error |
|---------|-----------|----------------|
| `01-no-domain-folder` | Missing `domain/` directory | "Domain path not found" |
| `02-missing-aggregates` | Commands exist but no aggregates | "No aggregates found for commands" |
| `03-no-event-decorators` | Events without `@Event` decorator | "Event missing @Event decorator" |
| `04-versioned-no-upcaster` | Event v2 exists without v1→v2 upcaster | "Missing upcaster for event version" |

### Python Valid Fixtures

| Fixture | Purpose | ADRs Demonstrated |
|---------|---------|-------------------|
| `01-todo-simple` | Simple Python VSA example | ADR-006 |

## Migration from vsa/examples

Fixtures in this directory were migrated from `/vsa/examples/`:

- `typescript/valid/01-hexagonal-complete/` ← from `vsa/examples/01-todo-list-ts/`
- `typescript/valid/02-multi-context/` ← from `vsa/examples/02-library-management-ts/`
- `python/valid/01-todo-simple/` ← from `vsa/examples/05-todo-list-py/`

Developer-facing examples now live in `/examples/` at the repository root.

## Related Documentation

- **TESTING-FRAMEWORK.md** - Overall testing strategy for vsa-core
- **PROJECT-PLAN_20251106_examples-migration.md** - Migration plan from vsa/examples
- **ADR-006** - Domain Organization Pattern
- **ADR-007** - Event Versioning and Upcasters
- **ADR-008** - Vertical Slices as Hexagonal Adapters

