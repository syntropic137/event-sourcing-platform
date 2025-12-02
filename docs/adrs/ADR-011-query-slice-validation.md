# ADR-011: Query Slice Validation

**Status:** Accepted
**Date:** 2025-12-02
**Decision Makers:** Architecture Team
**Related:** ADR-008 (Vertical Slices), ADR-009 (CQRS Pattern)

## Context

The VSA tool currently validates **command slices** (write operations) but lacks support for **query slices** (read operations). In a CQRS architecture, query slices have different requirements:

| Aspect | Command Slice | Query Slice |
|--------|--------------|-------------|
| Purpose | Change state | Retrieve data |
| Core files | Command, Handler, Event | Query, Projection, Handler |
| Returns | void / acknowledgment | Data |
| Dependencies | Aggregate, Event Store | Read Model, Projection Store |

### The Problem

Without query slice validation:
- ❌ Developers forget to create projections
- ❌ Projections don't subscribe to necessary events
- ❌ Cross-slice imports break isolation
- ❌ Adapters become bloated with business logic
- ❌ AI agents can't work on query slices independently

### Requirements

1. **Detect slice type** (command, query, saga) from contents or metadata
2. **Validate query slices** have required components
3. **Enforce isolation** - no cross-slice imports
4. **Keep adapters thin** - max line count

## Decision

We add query slice validation rules to the VSA tool:

### New Validation Rules

| Code | Rule | Severity | Description |
|------|------|----------|-------------|
| VSA007 | Query slice requires projection | Error | Query slices must have `*Projection.*` file |
| VSA008 | Query slice requires handler | Error | Query slices must have `*Handler.*` file |
| VSA009 | Projection has no event subscriptions | Warning | Projections should handle at least one event |
| VSA010 | Cross-slice import detected | Error | Slices cannot import from sibling slices |
| VSA011 | Adapter exceeds max lines | Warning | Controllers should be thin (<50 lines) |

### Slice Type Detection

**Implicit detection** (from file contents):
```
Has *Command.* file → command slice
Has *Query.* file → query slice
Has *Saga.* file → saga slice
```

**Explicit metadata** (`slice.yaml`):
```yaml
name: list-workflows
type: query
projection: WorkflowListProjection
subscribes_to:
  - WorkflowCreatedEvent
  - WorkflowCompletedEvent
returns: list[WorkflowSummary]
```

### Configuration

```yaml
# vsa.yaml
slices:
  types:
    command:
      require_handler: true
      require_event: true
      max_lines: 50
    query:
      require_projection: true
      require_handler: true
      max_lines: 50
    saga:
      require_event_subscription: true

validation:
  no_cross_slice_imports: true
  thin_adapters:
    max_lines: 50
    enforce: true
```

### CLI Scaffolding

```bash
# Generate query slice
vsa generate --context workflows --feature list-workflows --type query

# Generated structure:
slices/list_workflows/
├── WorkflowListProjection.py
├── ListWorkflowsHandler.py
├── ListWorkflowsController.py
├── test_list_workflows.py
└── slice.yaml
```

## Implementation

### 1. Projection Scanner

New scanner to detect projection files:

```rust
// vsa-core/src/scanners/projection_scanner.rs
pub struct ProjectionScanner<'a> {
    config: &'a ProjectionConfig,
    root: &'a Path,
}

impl<'a> ProjectionScanner<'a> {
    pub fn scan(&self) -> Result<Vec<Projection>> {
        // Find files matching *Projection.*
        // Parse subscribed events from file content
    }
}
```

### 2. Slice Type Scanner

```rust
// vsa-core/src/scanners/slice_scanner.rs
pub enum SliceType {
    Command,
    Query,
    Saga,
    Unknown,
}

pub fn detect_slice_type(feature_path: &Path) -> SliceType {
    // Check for slice.yaml first (explicit)
    if let Some(manifest) = read_slice_yaml(feature_path) {
        return manifest.slice_type;
    }

    // Otherwise detect from contents (implicit)
    let files = scan_feature_files(feature_path);

    if files.iter().any(|f| f.contains("Command")) {
        SliceType::Command
    } else if files.iter().any(|f| f.contains("Query")) {
        SliceType::Query
    } else if files.iter().any(|f| f.contains("Saga")) {
        SliceType::Saga
    } else {
        SliceType::Unknown
    }
}
```

### 3. Query Slice Validation

```rust
// vsa-core/src/validation/query_slice_rules.rs
pub struct RequireProjectionForQueryRule;

impl ValidationRule for RequireProjectionForQueryRule {
    fn code(&self) -> &str { "VSA007" }

    fn validate(&self, ctx: &ValidationContext, report: &mut Report) -> Result<()> {
        for feature in scan_features(ctx) {
            if detect_slice_type(&feature.path) == SliceType::Query {
                let has_projection = feature.files.iter()
                    .any(|f| f.name.contains("Projection"));

                if !has_projection {
                    report.errors.push(ValidationIssue {
                        code: self.code(),
                        path: feature.path,
                        message: format!(
                            "Query slice '{}' is missing a projection",
                            feature.name
                        ),
                        suggestions: vec![
                            Suggestion::create_file(
                                feature.path.join(format!("{}Projection.py", to_pascal(&feature.name))),
                                "Create projection to build read model from events"
                            )
                        ],
                    });
                }
            }
        }
        Ok(())
    }
}
```

### 4. Cross-Slice Import Detection

```rust
// vsa-core/src/validation/isolation_rules.rs
pub struct NoCrossSliceImportsRule;

impl ValidationRule for NoCrossSliceImportsRule {
    fn code(&self) -> &str { "VSA010" }

    fn validate(&self, ctx: &ValidationContext, report: &mut Report) -> Result<()> {
        for feature in scan_features(ctx) {
            for file in &feature.files {
                let imports = parse_imports(&file.path, ctx.language);

                for import in imports {
                    if is_sibling_slice_import(&import, &feature, ctx) {
                        report.errors.push(ValidationIssue {
                            code: self.code(),
                            path: file.path.clone(),
                            message: format!(
                                "Cross-slice import detected: '{}' imports from '{}'",
                                feature.name, import.source
                            ),
                            suggestions: vec![
                                Suggestion::manual(
                                    "Move shared code to domain/ or infrastructure/"
                                )
                            ],
                        });
                    }
                }
            }
        }
        Ok(())
    }
}
```

## Consequences

### Positive

1. **CQRS Enforcement** ✅
   - Query slices validated for required components
   - Clear separation of read/write concerns
   - Projections must subscribe to events

2. **Slice Isolation** ✅
   - No cross-slice imports enforced
   - Each slice is independently testable
   - AI agents can work in parallel

3. **Thin Adapters** ✅
   - Max line enforcement keeps adapters simple
   - Business logic stays in domain
   - Controllers remain as translation layers

4. **Better Developer Experience** ✅
   - CLI generates proper query slice structure
   - Early validation catches missing components
   - Clear error messages with suggestions

### Negative

1. **More Rules to Maintain** ⚠️
   - Six new validation rules
   - Import parsing for multiple languages
   - **Mitigation:** Good test coverage, modular design

2. **Configuration Complexity** ⚠️
   - New slice type configuration
   - Per-type settings
   - **Mitigation:** Sensible defaults, documentation

### Neutral

1. **Backward Compatibility**
   - Existing command slices continue to work
   - New rules can be disabled if needed
   - Query slice validation is opt-in initially

## Related ADRs

- ADR-008: Vertical Slices as Hexagonal Adapters
- ADR-009: CQRS Pattern Implementation
- ADR-006: Domain Organization Pattern

## References

- "CQRS Documents" - Greg Young
- "Vertical Slice Architecture" - Jimmy Bogard
- "Understanding Event Sourcing" - Alexey Zimarev

---

**Last Updated:** 2025-12-02
**Supersedes:** None
**Superseded By:** None
