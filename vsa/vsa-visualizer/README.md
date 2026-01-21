# VSA Visualizer

> **Generate beautiful architecture diagrams from VSA projects**

[![Tests](https://img.shields.io/badge/tests-91%20passing-brightgreen)]()
[![TypeScript](https://img.shields.io/badge/TypeScript-5.3-blue)]()
[![License](https://img.shields.io/badge/license-MIT-green)]()

VSA Visualizer is a command-line tool that automatically generates comprehensive architecture documentation with Mermaid diagrams from VSA (Vertical Slice Architecture) projects.

## ✨ Features

- 📊 **C4 Architecture Diagrams** - System context, containers, and components
- 🔄 **Event Flow Visualization** - See how events flow through your system
- 📦 **Aggregate Documentation** - Detailed pages for each aggregate
- 🔗 **Cross-Aggregate Flows** - Visualize sagas and process managers
- 🎨 **Mermaid Diagrams** - Beautiful, version-controllable diagrams
- 🚀 **CLI Integration** - Pipe directly from `vsa manifest`
- ✅ **Fully Tested** - 91 tests with comprehensive coverage

## 📦 Installation

```bash
cd vsa/vsa-visualizer
npm install
npm run build
```

Or use it directly via pipe without installation:

```bash
vsa manifest --include-domain | npx ts-node src/index.ts
```

## 🚀 Quick Start

### Generate from File

```bash
# Generate manifest first
vsa manifest --include-domain > manifest.json

# Generate documentation
vsa-visualizer manifest.json
```

### Pipe from vsa-cli (Recommended)

```bash
cd your-vsa-project
vsa manifest --include-domain | vsa-visualizer --output ./docs/architecture
```

### With Options

```bash
vsa-visualizer manifest.json \
  --output ./docs/arch \
  --format mermaid \
  --verbose
```

## 📖 Usage

### Command Line Options

```
Usage: vsa-visualizer [options] [manifest]

Arguments:
  manifest             Path to manifest JSON file or - for stdin (default: "-")

Options:
  -V, --version        output the version number
  -o, --output <dir>   Output directory (default: "docs/architecture")
  -f, --format <type>  Output format (currently only "mermaid") (default: "mermaid")
  -v, --verbose        Enable verbose logging (default: false)
  -h, --help           display help for command
```

### Examples

```bash
# Basic usage - reads from stdin by default
vsa manifest --include-domain | vsa-visualizer

# From file with custom output
vsa-visualizer manifest.json --output ./architecture-docs

# Verbose mode for debugging
vsa-visualizer manifest.json --verbose

# Show help
vsa-visualizer --help
```

## 📁 Generated Output

The visualizer creates a structured documentation directory:

```
docs/architecture/
├── OVERVIEW.md              # System overview with C4 diagrams
├── FLOWS.md                 # Cross-aggregate flows (if detected)
└── aggregates/
    ├── README.md            # Index of all aggregates
    ├── OrderAggregate.md    # Detailed aggregate page
    ├── CartAggregate.md
    └── ...
```

### OVERVIEW.md Contents

- System statistics
- C4 Context diagram
- C4 Container diagram (for multi-context systems)
- Aggregates overview with relationships
- Event flow visualization

### Aggregate Pages

Each aggregate gets a detailed page with:

- Commands table (handler methods, emitted events)
- Events table (versions, field counts)
- Command flow sequence diagram
- State transition diagram

### FLOWS.md (Optional)

Generated when cross-aggregate event flows are detected:

- Flow detection and analysis
- Sequence diagrams for event propagation
- Event handler details table

## 🔧 Manifest Requirements

The visualizer requires a VSA manifest with domain data:

```bash
vsa manifest --include-domain > manifest.json
```

### Required Manifest Fields

```json
{
  "version": "0.6.1-beta",
  "schema_version": "1.0.0",
  "generated_at": "2026-01-21T00:00:00Z",
  "domain": {
    "aggregates": [...],
    "commands": [...],
    "events": [...],
    "relationships": {
      "command_to_aggregate": {},
      "aggregate_to_events": {},
      "event_to_handlers": {}
    }
  }
}
```

See [Manifest Schema](#manifest-schema) for full details.

## 🏗️ Architecture

```
┌─────────────────┐
│   vsa-cli       │  Generates JSON manifest
│   (Rust)        │  with --include-domain flag
└────────┬────────┘
         │ JSON Manifest
         │ (versioned contract)
         ▼
┌─────────────────────────────────────┐
│  vsa-visualizer (TypeScript)        │
│                                     │
│  ┌─────────────────────────────┐   │
│  │  Manifest Parser            │   │
│  │  - Validate schema          │   │
│  │  - Type safety              │   │
│  └─────────────────────────────┘   │
│              │                      │
│              ▼                      │
│  ┌─────────────────────────────┐   │
│  │  Generators                 │   │
│  │  - OverviewGenerator        │   │
│  │  - AggregateGenerator       │   │
│  │  - FlowsGenerator           │   │
│  └─────────────────────────────┘   │
│              │                      │
│              ▼                      │
│  ┌─────────────────────────────┐   │
│  │  Markdown + Mermaid Output  │   │
│  └─────────────────────────────┘   │
└─────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────┐
│  docs/architecture/                 │
│  - OVERVIEW.md                      │
│  - aggregates/*.md                  │
│  - FLOWS.md                         │
└─────────────────────────────────────┘
```

## 🧪 Development

### Prerequisites

- Node.js 18+ 
- npm or pnpm
- VSA CLI (for testing)

### Setup

```bash
# Install dependencies
npm install

# Build TypeScript
npm run build

# Run tests
npm test

# Run tests with coverage
npm test -- --coverage

# Type check
npm run type-check

# Lint and format
npm run lint
npm run format
```

### Running Tests

```bash
# All tests
npm test

# Watch mode
npm test -- --watch

# Specific test file
npm test -- parser.test.ts

# Integration tests only
npm test -- integration
```

### Project Structure

```
vsa-visualizer/
├── src/
│   ├── index.ts                    # CLI entry point
│   ├── types/
│   │   └── manifest.ts             # TypeScript types
│   ├── manifest/
│   │   └── parser.ts               # Manifest validation
│   ├── generators/
│   │   ├── base-generator.ts       # Base class
│   │   ├── overview-generator.ts   # System overview
│   │   ├── aggregate-generator.ts  # Aggregate details
│   │   └── flows-generator.ts      # Cross-aggregate flows
│   └── utils/
│       └── file-writer.ts          # File operations
├── tests/
│   ├── fixtures/                   # Test data
│   ├── manifest/                   # Parser tests
│   ├── generators/                 # Generator tests
│   ├── utils/                      # Utility tests
│   └── integration/                # End-to-end tests
└── dist/                           # Compiled JavaScript
```

## 📚 Manifest Schema

### Complete Schema

<details>
<summary>Click to expand full manifest type definitions</summary>

```typescript
interface Manifest {
  version: string;
  schema_version: string;
  generated_at: string;
  domain?: DomainManifest;
  bounded_contexts?: BoundedContext[];
}

interface DomainManifest {
  aggregates: Aggregate[];
  commands: Command[];
  events: Event[];
  queries?: Query[];
  upcasters?: Upcaster[];
  relationships: Relationships;
}

// See src/types/manifest.ts for complete definitions
```

</details>

### Schema Version Compatibility

| Schema Version | Visualizer Version | Notes |
|----------------|-------------------|-------|
| 1.0.0          | 0.1.0+           | Initial release |

## 🔍 Troubleshooting

### "Manifest does not contain domain data"

**Solution**: Regenerate manifest with `--include-domain` flag:

```bash
vsa manifest --include-domain > manifest.json
```

### "Invalid JSON" error

**Cause**: Mixing stdout and stderr from vsa-cli

**Solution**: The latest vsa-cli outputs status to stderr, so piping should work:

```bash
vsa manifest --include-domain | vsa-visualizer
```

If still having issues, save to file first:

```bash
vsa manifest --include-domain > manifest.json
vsa-visualizer manifest.json
```

### Empty or incomplete diagrams

**Cause**: VSA scanner may not be detecting all domain objects

**Solution**: 
1. Ensure your code follows VSA naming conventions
2. Check that decorators are present on events
3. Verify aggregate command/event handlers are public
4. Run with `--verbose` to see what was detected

### Mermaid diagrams not rendering on GitHub

**Cause**: GitHub requires specific Mermaid syntax

**Solution**: The visualizer generates GitHub-compatible Mermaid. If diagrams don't render:
1. Ensure you're viewing on github.com (not raw)
2. Check that the file has `.md` extension
3. Try refreshing the page

## 🎯 Roadmap

- [x] **M2**: Bootstrap TypeScript project
- [x] **M3**: Overview generator with C4 diagrams
- [x] **M4**: Aggregate documentation generator
- [x] **M5**: Cross-aggregate flows generator
- [x] **M6**: CLI implementation and integration
- [x] **M7**: Testing and documentation
- [ ] **M8**: VSA monorepo integration

### Future Enhancements

- [ ] Interactive HTML output
- [ ] PlantUML export option
- [ ] Filtering by context/aggregate
- [ ] Diff visualization
- [ ] VS Code extension
- [ ] Watch mode for real-time updates
- [ ] Custom diagram templates

## 🤝 Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for guidelines.

This tool is part of the VSA monorepo. Major changes should:
1. Maintain backward compatibility with schema version 1.0.0
2. Include comprehensive tests
3. Update documentation
4. Follow the existing code style

## 📄 License

MIT License - see [LICENSE](../../LICENSE) for details

## 🙏 Acknowledgments

Built with:
- [Commander.js](https://github.com/tj/commander.js) - CLI framework
- [Mermaid](https://mermaid.js.org/) - Diagram generation
- [TypeScript](https://www.typescriptlang.org/) - Type safety
- [Jest](https://jestjs.io/) - Testing framework

---

**Part of the [VSA (Vertical Slice Architecture) Platform](../../README.md)**
