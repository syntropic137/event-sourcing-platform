---
sidebar_position: 2
---

# Configuration

Complete reference for `vsa.yaml` configuration file.

## Overview

The `vsa.yaml` file is the central configuration for your VSA project. It defines:
- Project language and structure
- Bounded contexts
- Integration events
- Validation rules
- Framework integration

## Basic Configuration

### Minimal Example

```yaml
version: 1
language: typescript
root: src/contexts
```

This is enough to get started!

### Complete Example

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
    description: Order processing and management
    publishes:
      - OrderPlaced
      - OrderCancelled
    subscribes:
      - ProductAdded
      - StockAdjusted
    validation:
      require_aggregate: true
  
  - name: inventory
    description: Stock management
    publishes:
      - StockAdjusted
    subscribes:
      - OrderPlaced

integration_events:
  path: ../_shared/integration-events
  events:
    OrderPlaced:
      publisher: orders
      subscribers: [inventory, shipping]
      description: "Customer placed an order"
      version: 1

validation:
  require_tests: true
  require_handler: true
  require_aggregate: false
  exclude:
    - "**/*.generated.ts"
    - "legacy/**"
```

## Configuration Fields

### `version`

**Type:** `number`  
**Required:** Yes  
**Default:** N/A

Configuration schema version. Currently only `1` is supported.

```yaml
version: 1
```

### `language`

**Type:** `string`  
**Required:** Yes  
**Options:** `typescript`, `python`, `rust`

Primary language for the project.

```yaml
language: typescript
```

### `root`

**Type:** `string`  
**Required:** Yes  
**Default:** `./src/contexts`

Root directory containing bounded contexts (relative to config file).

```yaml
root: src/contexts
# or
root: ./vertical-slices
```

## Framework Integration

### `framework`

**Type:** `object`  
**Required:** No

Enables integration with event sourcing frameworks.

```yaml
framework:
  name: event-sourcing-platform
  aggregate_class: AggregateRoot
  aggregate_import: "@event-sourcing-platform/typescript"
```

#### Fields

- **`name`** (`string`) - Framework name
- **`aggregate_class`** (`string`) - Base aggregate class to extend
- **`aggregate_import`** (`string`) - Import path for aggregate class

#### Example with Custom Framework

```yaml
framework:
  name: my-framework
  aggregate_class: BaseAggregate
  aggregate_import: "@my-company/event-sourcing"
```

## Bounded Contexts

### `bounded_contexts`

**Type:** `array`  
**Required:** No

List of bounded contexts in your system.

```yaml
bounded_contexts:
  - name: orders
    description: Order processing
    publishes: [OrderPlaced]
    subscribes: [ProductAdded]
  
  - name: inventory
    description: Stock management
    publishes: [StockAdjusted]
    subscribes: [OrderPlaced]
```

#### Context Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | `string` | ✅ | Context name (kebab-case) |
| `description` | `string` | ❌ | Human-readable description |
| `publishes` | `array` | ❌ | Integration events this context publishes |
| `subscribes` | `array` | ❌ | Integration events this context subscribes to |
| `validation` | `object` | ❌ | Context-specific validation rules |

#### Context-Specific Validation

Override global validation for a specific context:

```yaml
bounded_contexts:
  - name: orders
    validation:
      require_aggregate: true  # Orders requires aggregates
      require_tests: true
  
  - name: notifications
    validation:
      require_aggregate: false  # Notifications don't need aggregates
      require_tests: false  # Skip tests for now
```

## Integration Events

### `integration_events`

**Type:** `object`  
**Required:** No

Configuration for integration events (events that cross context boundaries).

```yaml
integration_events:
  path: ../_shared/integration-events
  
  events:
    OrderPlaced:
      publisher: orders
      subscribers: [inventory, shipping, notifications]
      description: "Customer placed an order"
      version: 1
    
    StockAdjusted:
      publisher: inventory
      subscribers: [orders]
      description: "Product stock quantity changed"
      version: 1
```

#### Fields

- **`path`** (`string`) - Path to integration events directory (relative to root)
- **`events`** (`object`) - Map of event names to event configurations

#### Event Configuration

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `publisher` | `string` | ✅ | Context that publishes this event |
| `subscribers` | `array` | ✅ | Contexts that subscribe to this event |
| `description` | `string` | ❌ | Event description |
| `version` | `number` | ❌ | Event schema version |

## Validation Rules

### `validation`

**Type:** `object`  
**Required:** No

Global validation rules for all features.

```yaml
validation:
  require_tests: true
  require_handler: true
  require_aggregate: false
  exclude:
    - "**/*.generated.ts"
    - "legacy/**"
```

#### Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `require_tests` | `boolean` | `true` | Every feature must have tests |
| `require_handler` | `boolean` | `true` | Every command needs a handler |
| `require_aggregate` | `boolean` | `false` | Every feature needs an aggregate |
| `exclude` | `array` | `[]` | Glob patterns to exclude from validation |

#### Examples

**Strict validation:**

```yaml
validation:
  require_tests: true
  require_handler: true
  require_aggregate: true
```

**Relaxed validation:**

```yaml
validation:
  require_tests: false
  require_handler: true
  require_aggregate: false
```

**With exclusions:**

```yaml
validation:
  require_tests: true
  exclude:
    - "**/legacy/**"
    - "**/*.generated.ts"
    - "**/temp/**"
```

## File Structure

Where `vsa.yaml` fits in your project:

```
my-project/
├── vsa.yaml                 ← Configuration file
├── package.json
├── src/
│   ├── contexts/            ← Root directory (from config)
│   │   ├── orders/
│   │   ├── inventory/
│   │   └── shipping/
│   └── _shared/
│       └── integration-events/  ← Integration events path
└── tests/
```

## Environment-Specific Configuration

### Development vs Production

**Option 1: Multiple config files**

```bash
# Development
vsa --config vsa.dev.yaml validate

# Production
vsa --config vsa.prod.yaml validate
```

**vsa.dev.yaml:**
```yaml
version: 1
language: typescript
root: src/contexts

validation:
  require_tests: false  # Relaxed for dev
  require_aggregate: false
```

**vsa.prod.yaml:**
```yaml
version: 1
language: typescript
root: src/contexts

validation:
  require_tests: true  # Strict for prod
  require_aggregate: true
```

### Monorepo Setup

**Option: Per-package configuration**

```
monorepo/
├── packages/
│   ├── orders/
│   │   ├── vsa.yaml         ← Orders config
│   │   └── src/contexts/
│   └── inventory/
│       ├── vsa.yaml         ← Inventory config
│       └── src/contexts/
```

## Examples by Use Case

### Microservices

```yaml
version: 1
language: typescript
root: src

bounded_contexts:
  - name: orders
    # Only one context per service
    
validation:
  require_tests: true
  require_aggregate: true
```

### Modular Monolith

```yaml
version: 1
language: typescript
root: src/contexts

bounded_contexts:
  - name: orders
  - name: inventory
  - name: shipping
  - name: notifications
  
validation:
  require_tests: true
  require_aggregate: true
```

### CQRS with Separate Read Models

```yaml
version: 1
language: typescript
root: src/contexts

bounded_contexts:
  - name: orders
    publishes: [OrderPlaced]
    validation:
      require_aggregate: true  # Write side needs aggregates
  
  - name: orders-queries
    subscribes: [OrderPlaced]
    validation:
      require_aggregate: false  # Read side doesn't need aggregates
```

## Best Practices

### ✅ Do's

- Keep configuration minimal
- Use descriptive context names
- Document integration events
- Use validation to enforce standards
- Version your configuration file

### ❌ Don'ts

- Don't add unused contexts
- Don't disable all validation
- Don't use generic context names
- Don't forget to document events
- Don't commit sensitive data

## Migration

### From No Configuration

```bash
# 1. Initialize
vsa init

# 2. Edit generated vsa.yaml
# Add your contexts

# 3. Validate
vsa validate
```

### Updating Configuration

```bash
# 1. Edit vsa.yaml

# 2. Validate changes
vsa validate

# 3. Generate manifest to verify
vsa manifest
```

## Troubleshooting

### Configuration Not Found

```bash
Error: Configuration file not found: vsa.yaml

# Solution: Initialize or specify path
vsa init
# or
vsa --config path/to/vsa.yaml validate
```

### Invalid Configuration

```bash
Error: Invalid configuration: missing required field 'version'

# Solution: Add required fields
version: 1
language: typescript
root: src/contexts
```

### Context Not Found

```bash
Error: Context 'orders' not found in configuration

# Solution: Add context to bounded_contexts
bounded_contexts:
  - name: orders
    description: Order processing
```

## Schema Validation

VSA validates configuration on load:

```bash
$ vsa validate

❌ Configuration Error:
   File: vsa.yaml
   Error: Invalid language 'javascript'
   Valid options: typescript, python, rust
```

## Next Steps

- **[Commands](./commands)** - Learn CLI commands
- **[Validation Rules](./validation-rules)** - Understand validation
- **[VS Code Extension](../ide/vscode-extension)** - IDE integration with YAML schema

---

**Need more examples?** Check out the [Examples](../examples/overview) section for real-world configurations.

