---
sidebar_position: 1
---

# CLI Commands

Complete reference for all VSA CLI commands.

## Overview

The VSA CLI provides commands for scaffolding, validating, and managing vertical slice architectures. All commands follow the pattern:

```bash
vsa [OPTIONS] <COMMAND> [ARGS]
```

### Global Options

These options work with any command:

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--config` | `-c` | Path to config file | `vsa.yaml` |
| `--verbose` | `-v` | Enable verbose logging | `false` |
| `--help` | `-h` | Show help information | - |
| `--version` | `-V` | Show version | - |

### Examples

```bash
# Use custom config file
vsa --config my-config.yaml validate

# Enable verbose output
vsa --verbose generate --context orders --feature place-order

# Show version
vsa --version
```

## Commands

### `init` - Initialize VSA Configuration

Creates a new VSA project with configuration file and directory structure.

#### Usage

```bash
vsa init [OPTIONS]
```

#### Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--root` | `-r` | Root directory for contexts | `./src/contexts` |
| `--language` | `-l` | Primary language | `typescript` |
| `--with-framework` | - | Enable framework integration | `false` |

#### Examples

```bash
# Basic initialization (TypeScript)
vsa init

# With custom root directory
vsa init --root src/vertical-slices

# With Python
vsa init --language python

# With event-sourcing framework integration
vsa init --with-framework
```

#### Output

Creates:
- `vsa.yaml` - Configuration file
- `src/contexts/` - Root directory for contexts
- `.vscode/settings.json` - VS Code configuration (if detected)

```yaml
# Generated vsa.yaml
version: 1
language: typescript
root: src/contexts

bounded_contexts: []

validation:
  require_tests: true
  require_handler: true
  require_aggregate: false
```

---

### `generate` - Generate New Feature

Scaffolds a complete vertical slice with command, event, handler, and tests.

#### Usage

```bash
vsa generate [OPTIONS] --context <CONTEXT> --feature <FEATURE>
```

#### Options

| Option | Short | Description | Required |
|--------|-------|-------------|----------|
| `--context` | `-c` | Context name | ✅ |
| `--feature` | `-f` | Feature name (kebab-case) | ✅ |
| `--feature-type` | `-t` | Feature type (command, query, event) | ❌ |
| `--interactive` | `-i` | Interactive mode with prompts | ❌ |

#### Examples

```bash
# Generate basic feature
vsa generate --context orders --feature place-order

# Interactive mode (prompts for details)
vsa generate --context orders --feature place-order --interactive

# Short form
vsa generate -c orders -f cancel-order

# Generate query feature
vsa generate --context orders --feature get-order --feature-type query
```

#### Interactive Mode

When using `--interactive`, VSA prompts for:
- Fields (name, type, required)
- Aggregate inclusion
- Integration events

```bash
$ vsa generate -c orders -f place-order --interactive

🚧 Generating feature 'place-order' in context 'orders'...

📋 Let's configure your feature

Field name (or press Enter to finish): orderId
Field type [string]: string
Is this field required? [Y/n]: Y
  ✓ Added field: orderId (string, required)

Field name (or press Enter to finish): customerId
Field type [string]: string
Is this field required? [Y/n]: Y
  ✓ Added field: customerId (string, required)

Field name (or press Enter to finish): items
Field type [string]: OrderItem[]
Is this field required? [Y/n]: Y
  ✓ Added field: items (OrderItem[], required)

Field name (or press Enter to finish): 

Include aggregate? [y/N]: Y
Aggregate name [PlaceOrderAggregate]: OrderAggregate

Does this feature publish integration events? [y/N]: Y
ℹ Integration events will need to be defined in _shared/integration-events/

✅ Created feature files:
  ├─ src/contexts/orders/place-order/PlaceOrderCommand.ts
  ├─ src/contexts/orders/place-order/OrderPlacedEvent.ts
  ├─ src/contexts/orders/place-order/PlaceOrderHandler.ts
  ├─ src/contexts/orders/place-order/OrderAggregate.ts
  └─ src/contexts/orders/place-order/PlaceOrder.test.ts

💡 Next steps:
  1. Implement business logic in PlaceOrderHandler
  2. Add tests in PlaceOrder.test.ts
  3. Run: vsa validate
```

#### Generated Files

```typescript
// PlaceOrderCommand.ts
export interface PlaceOrderCommand {
  orderId: string;
  customerId: string;
  items: OrderItem[];
}

// OrderPlacedEvent.ts
export interface OrderPlacedEvent {
  orderId: string;
  customerId: string;
  items: OrderItem[];
  placedAt: Date;
}

// PlaceOrderHandler.ts
export class PlaceOrderHandler {
  async handle(command: PlaceOrderCommand): Promise<void> {
    // TODO: Implement business logic
  }
}

// PlaceOrder.test.ts
describe('PlaceOrder', () => {
  it('should place an order successfully', async () => {
    // TODO: Implement test
  });
});
```

---

### `validate` - Validate VSA Structure

Validates that your code follows VSA conventions and rules.

#### Usage

```bash
vsa validate [OPTIONS]
```

#### Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--fix` | - | Auto-fix issues when possible | `false` |
| `--watch` | `-w` | Watch for changes and re-validate | `false` |

#### Examples

```bash
# Validate once
vsa validate

# Watch mode (auto-validates on file changes)
vsa validate --watch

# Auto-fix issues
vsa validate --fix

# Watch mode with verbose output
vsa --verbose validate --watch
```

#### Output

```bash
$ vsa validate

✅ Validating: src/contexts

📦 Context: orders
  ✅ place-order
     ├─ PlaceOrderCommand.ts
     ├─ OrderPlacedEvent.ts
     ├─ PlaceOrderHandler.ts
     └─ PlaceOrder.test.ts

  ❌ cancel-order
     ├─ CancelOrderCommand.ts
     ├─ OrderCancelledEvent.ts
     └─ ✗ Missing: CancelOrderHandler.ts
     
📦 Context: inventory
  ✅ adjust-stock
     ├─ AdjustStockCommand.ts
     ├─ StockAdjustedEvent.ts
     ├─ AdjustStockHandler.ts
     └─ AdjustStock.test.ts

❌ Validation Summary:
  2 features validated
  1 error found
  
💡 Issues:
  1. contexts/orders/cancel-order/
     Missing required file: CancelOrderHandler.ts
     Required by: validation.require_handler = true
```

#### Exit code

`vsa validate` (without `--watch`, which keeps running) exits non-zero when either of these holds:

- there are errors and `validation.fail_on_errors` is `true` (the default)
- there are more warnings than `validation.max_warnings`

`max_warnings` has no default. If it is omitted, warnings are reported but never fail the run. Set `max_warnings: 0` to make every warning blocking, which is what a CI gate usually wants:

```yaml
validation:
  max_warnings: 0
  fail_on_errors: true
```

#### Validation Rules

VSA validates:

1. **File Naming Conventions**
   - Commands end with `Command.{ext}`
   - Events end with `Event.{ext}`
   - Handlers end with `Handler.{ext}`
   - Tests end with `.test.{ext}`

2. **Required Files**
   - Every feature has a command
   - Handler exists (if `require_handler: true`)
   - Tests exist (if `require_tests: true`)
   - Aggregate exists (if `require_aggregate: true`)

3. **Bounded Context Rules**
   - No direct cross-context imports
   - Integration events in `_shared/integration-events/`
   - Subscribers in `_subscribers/` folder
   - Publishers match configuration

4. **Special Folders**
   - `_subscribers/` contains `*.handler.{ext}`
   - `_shared/` for context-internal code only
   - `_queries/` for query operations

#### Watch Mode

Keep validation running during development:

```bash
$ vsa validate --watch

👀 Watching: src/contexts
   Press Ctrl+C to stop

✅ Initial validation passed (2 features, 0 errors)

[12:34:56] File changed: src/contexts/orders/place-order/PlaceOrderHandler.ts
[12:34:56] ✅ Validating...
[12:34:56] ✅ Validation passed (2 features, 0 errors)

[12:35:23] File changed: src/contexts/orders/cancel-order/CancelOrderCommand.ts
[12:35:23] ✅ Validating...
[12:35:23] ❌ Validation failed: Missing CancelOrderHandler.ts
```

---

### `list` - List Contexts and Features

Shows all bounded contexts and their features.

#### Usage

```bash
vsa list [OPTIONS]
```

#### Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--contexts-only` | - | Show only contexts | `false` |
| `--context` | `-c` | Filter by context | `null` |
| `--format` | `-f` | Output format | `tree` |

#### Formats

- `tree` - Tree view (default)
- `text` - Simple list
- `json` - JSON output

#### Examples

```bash
# Tree view (default)
vsa list

# Only show contexts
vsa list --contexts-only

# Filter by context
vsa list --context orders

# JSON output
vsa list --format json

# Text list
vsa list --format text
```

#### Output Examples

**Tree Format:**

```bash
$ vsa list

📦 VSA Project Structure

contexts/
├── 📁 orders (Order processing)
│   ├── ✨ place-order
│   │   ├─ PlaceOrderCommand.ts
│   │   ├─ OrderPlacedEvent.ts
│   │   ├─ PlaceOrderHandler.ts
│   │   └─ PlaceOrder.test.ts
│   ├── ✨ cancel-order
│   │   ├─ CancelOrderCommand.ts
│   │   ├─ OrderCancelledEvent.ts
│   │   ├─ CancelOrderHandler.ts
│   │   └─ CancelOrder.test.ts
│   └── 📂 _subscribers
│       └─ ProductAdded.handler.ts
│
├── 📁 inventory (Stock management)
│   ├── ✨ adjust-stock
│   │   ├─ AdjustStockCommand.ts
│   │   ├─ StockAdjustedEvent.ts
│   │   ├─ AdjustStockHandler.ts
│   │   └─ AdjustStock.test.ts
│   └── 📂 _subscribers
│       └─ OrderPlaced.handler.ts
│
└── 📁 shipping (Shipment tracking)
    └── ✨ create-shipment
        ├─ CreateShipmentCommand.ts
        ├─ ShipmentCreatedEvent.ts
        ├─ CreateShipmentHandler.ts
        └─ CreateShipment.test.ts

Summary:
  3 contexts
  4 features
  2 integration event handlers
```

**Text Format:**

```bash
$ vsa list --format text

Contexts:
  - orders
  - inventory
  - shipping

Features:
  orders/place-order
  orders/cancel-order
  inventory/adjust-stock
  shipping/create-shipment
```

**JSON Format:**

```bash
$ vsa list --format json

{
  "contexts": [
    {
      "name": "orders",
      "description": "Order processing",
      "features": [
        {
          "name": "place-order",
          "path": "src/contexts/orders/place-order",
          "files": ["PlaceOrderCommand.ts", "OrderPlacedEvent.ts", "PlaceOrderHandler.ts", "PlaceOrder.test.ts"]
        },
        {
          "name": "cancel-order",
          "path": "src/contexts/orders/cancel-order",
          "files": ["CancelOrderCommand.ts", "OrderCancelledEvent.ts", "CancelOrderHandler.ts", "CancelOrder.test.ts"]
        }
      ],
      "subscribers": ["ProductAdded.handler.ts"]
    }
  ]
}
```

---

### `manifest` - Generate Architecture Manifest

Generates documentation of your VSA architecture.

#### Usage

```bash
vsa manifest [OPTIONS]
```

#### Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--output` | `-o` | Output file path | `stdout` |
| `--format` | `-f` | Output format | `json` |

#### Formats

- `json` - JSON format
- `yaml` - YAML format
- `markdown` - Markdown document (future)

#### Examples

```bash
# Output to stdout
vsa manifest

# Save to file
vsa manifest --output architecture.json

# YAML format
vsa manifest --format yaml --output architecture.yaml

# Markdown format (future)
vsa manifest --format markdown --output ARCHITECTURE.md
```

#### Rust and `hexagonal-event-sourced-vsa` (schema 2.4.0)

- A Rust command or event file that declares a `*Command` / `*Event` enum gives one entry per variant, named `Enum::Variant` (`OrderCommand::Place`), with its fields (`Option<_>` fields are optional; tuple fields are named by position). Other files keep one entry per file.
- `relationships.command_to_events` maps each command variant to the event variants its decision builds, read from the `Command::Variant =>` arms in the aggregate's folder and the functions those arms call. `command_to_aggregate` and `aggregate_to_events` are filled from the same reading. Macros, trait dispatch and events built outside the domain folder are not followed.
- Under `hexagonal-event-sourced-vsa`, a context's `features` are its slices (`<context>/slices/<name>/`, as validation sees them), each with `slice_type` (the primary type) and `slice_types` (every declared pattern). Layer folders (`application/`, `ports/`, `infrastructure/`, `tests/`) and a slice's subfolders are not features. Other architectures keep the directory-based features.
- An aggregate's Rust `entities` are files in its folder that declare a struct with an `id` / `*_id` field.

#### Output

```json
{
  "version": "1.0.0",
  "language": "typescript",
  "contexts": [
    {
      "name": "orders",
      "description": "Order processing",
      "features": [
        {
          "name": "place-order",
          "command": "PlaceOrderCommand",
          "event": "OrderPlacedEvent",
          "handler": "PlaceOrderHandler",
          "aggregate": null,
          "tests": ["PlaceOrder.test.ts"]
        }
      ],
      "publishes": ["OrderPlaced", "OrderCancelled"],
      "subscribes": ["ProductAdded", "StockAdjusted"],
      "subscribers": [
        {
          "event": "ProductAdded",
          "handler": "ProductAdded.handler.ts"
        }
      ]
    }
  ],
  "integrationEvents": [
    {
      "name": "OrderPlaced",
      "publisher": "orders",
      "subscribers": ["inventory", "shipping"]
    }
  ]
}
```

---

## Common Workflows

### Starting a New Project

```bash
# 1. Initialize
vsa init --language typescript

# 2. Edit vsa.yaml to define contexts

# 3. Generate first feature
vsa generate --context orders --feature place-order --interactive

# 4. Validate
vsa validate

# 5. Start watch mode for continuous validation
vsa validate --watch
```

### Adding a New Feature

```bash
# 1. Generate scaffolding
vsa generate -c orders -f cancel-order

# 2. Implement logic in generated files

# 3. Validate
vsa validate

# 4. List to verify
vsa list --context orders
```

### Daily Development

```bash
# Terminal 1: Watch mode
vsa validate --watch

# Terminal 2: Your development
# Edit files, validation happens automatically
```

### Generating Documentation

```bash
# Generate architecture manifest
vsa manifest --format json --output docs/architecture.json

# List features for README
vsa list --format text >> README.md

# Create context overview
vsa list --contexts-only
```

## Tips & Tricks

### Use Aliases

```bash
# Add to ~/.bashrc or ~/.zshrc
alias vsav='vsa validate'
alias vsag='vsa generate'
alias vsal='vsa list'
alias vsam='vsa manifest'
```

### Combine with Other Tools

```bash
# Validate before committing
git add . && vsa validate && git commit

# Generate and open in editor
vsa generate -c orders -f place-order && code src/contexts/orders/place-order

# Watch validation and tests together
vsa validate --watch & npm test -- --watch
```

### JSON Output for Scripting

```bash
# Parse with jq
vsa list --format json | jq '.contexts[].name'

# Count features
vsa manifest | jq '.contexts[].features | length'

# List all integration events
vsa manifest | jq '.integrationEvents[].name'
```

## Next Steps

- **[Configuration](./configuration)** - Configure vsa.yaml
- **[Validation Rules](./validation-rules)** - All validation rules explained
- **[Watch Mode](./watch-mode)** - Real-time validation details

---

**Need help?** Run `vsa --help` or `vsa <command> --help` for detailed usage information.

