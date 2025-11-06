# VSA - Vertical Slice Architecture for VS Code

VS Code extension for the Vertical Slice Architecture (VSA) Manager. Provides real-time validation, code generation, and architectural guidance for projects using vertical slice architecture with bounded contexts.

## Features

### ✅ Real-time Validation
- Validates vertical slice structure on file save
- Shows errors and warnings inline in the editor
- Configurable validation rules

### 🎨 Code Generation
- Generate new features with scaffolding
- Interactive prompts for feature details
- Framework integration support

### 📊 Architecture Insights
- List all features in the workspace
- Generate manifest files
- Validate bounded context boundaries

### 🔧 Commands

- **VSA: Validate Architecture** - Run validation manually
- **VSA: Generate Feature** - Generate a new vertical slice feature
- **VSA: List Features** - Show all features in the workspace
- **VSA: Generate Manifest** - Generate a JSON manifest of your architecture

## Requirements

This extension requires the `vsa` CLI tool to be installed and available in your PATH.

Install the CLI tool:

```bash
cargo install vsa-cli
```

Or build from source:

```bash
cd vsa/vsa-cli
cargo build --release
cp target/release/vsa /usr/local/bin/
```

## Configuration

The extension looks for a `vsa.yaml` file in the workspace root. Create one with:

```bash
vsa init
```

### Extension Settings

- `vsa.validateOnSave`: Run validation when files are saved (default: `true`)
- `vsa.validateOnOpen`: Run validation when workspace is opened (default: `true`)
- `vsa.configPath`: Path to vsa.yaml config file (default: `vsa.yaml`)
- `vsa.showWarnings`: Show validation warnings (default: `true`)

## Usage

### 1. Initialize VSA in Your Project

```bash
vsa init
```

### 2. Open Project in VS Code

The extension will automatically activate when it detects a `vsa.yaml` file.

### 3. Validate Your Architecture

Save any file or run the command:

```
Cmd/Ctrl + Shift + P → VSA: Validate Architecture
```

### 4. Generate New Features

```
Cmd/Ctrl + Shift + P → VSA: Generate Feature
```

Enter the feature path (e.g., `warehouse/products/create-product`) and follow the prompts.

## Example vsa.yaml

```yaml
version: 1
language: typescript
root: src/contexts

framework:
  name: event-sourcing-platform
  aggregate_class: AggregateRoot
  aggregate_import: "@event-sourcing-platform/typescript"
  event_class: DomainEvent
  event_import: "@event-sourcing-platform/typescript"

bounded_contexts:
  - name: warehouse
    description: Manages physical inventory
    publishes:
      - ProductStockChanged
    subscribes:
      - OrderPlaced

  - name: sales
    description: Manages customer orders
    publishes:
      - OrderPlaced
    subscribes:
      - ProductStockChanged

integration_events:
  path: ../_shared/integration-events
```

## Validation Rules

The extension enforces the following conventions:

1. **Naming Conventions**: Files must follow PascalCase + suffix pattern
   - Commands: `*Command.ts`
   - Events: `*Event.ts`
   - Handlers: `*Handler.ts`
   - Aggregates: `*Aggregate.ts`
   - Tests: `*.test.ts`

2. **Required Files**: Each operation must have:
   - One Command file
   - One Handler file
   - One Test file

3. **Bounded Context Boundaries**: 
   - No direct imports between contexts
   - Use integration events for cross-context communication

4. **Integration Events**:
   - Single source of truth in `_shared/integration-events/`
   - Must be declared in `vsa.yaml`

## Troubleshooting

### Extension Not Activating

Make sure you have a `vsa.yaml` file in your workspace root.

### VSA CLI Not Found

Install the CLI tool and ensure it's in your PATH:

```bash
which vsa
```

### Validation Errors Not Showing

Check the Output panel (View → Output → VSA) for error messages.

## Contributing

See the main [VSA repository](https://github.com/neuralempowerment/event-sourcing-platform) for contribution guidelines.

## License

MIT

