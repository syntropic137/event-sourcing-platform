# VSA Visualizer

Generate architecture diagrams from VSA (Vertical Slice Architecture) projects.

## Overview

VSA Visualizer is a tool that consumes the JSON manifest output from `vsa-cli` and generates beautiful Mermaid diagrams for documentation. It creates:

- **OVERVIEW.md**: System-level C4 diagrams and bounded context visualization
- **aggregates/*.md**: Detailed view of each aggregate with commands, events, and flows
- **FLOWS.md**: Cross-aggregate event flows and sagas (when present)

## Installation

```bash
cd vsa/vsa-visualizer
npm install
npm run build
```

## Usage

### Basic Usage

Generate diagrams from a manifest file:

```bash
vsa-visualizer manifest.json
```

### With Custom Output Directory

```bash
vsa-visualizer manifest.json --output docs/architecture
```

### Pipe from vsa-cli

```bash
cd examples/002-simple-aggregate-ts
vsa manifest --include-domain | vsa-visualizer --output ./docs/architecture
```

### Verbose Mode

```bash
vsa-visualizer manifest.json --verbose
```

## Manifest Requirements

The visualizer requires a VSA manifest that includes domain data. Generate one with:

```bash
vsa manifest --include-domain > manifest.json
```

The manifest must have:
- `schema_version`: "1.0.0" (or compatible)
- `domain`: Object containing aggregates, commands, events, and relationships

## Development

### Build

```bash
npm run build
```

### Run Tests

```bash
npm test
```

### Type Check

```bash
npm run type-check
```

### Development Mode

```bash
npm run dev -- manifest.json
```

## Architecture

```
┌─────────────────┐
│   vsa-cli       │  Generates JSON manifest with --include-domain
│   (Rust)        │
└────────┬────────┘
         │ JSON Manifest (versioned contract)
         ▼
┌─────────────────────────────────────┐
│  vsa-visualizer (TypeScript)        │
│  ├── src/                           │
│  │   ├── manifest/parser.ts         │  Parse and validate manifest
│  │   ├── generators/                │  Generate diagrams
│  │   ├── templates/                 │  Markdown formatting
│  │   └── utils/file-writer.ts       │  Write output files
└─────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────┐
│  docs/architecture/                 │  Generated markdown with Mermaid
│  ├── OVERVIEW.md                    │
│  ├── aggregates/*.md                │
│  └── FLOWS.md                       │
└─────────────────────────────────────┘
```

## Roadmap

### Milestone 2: Bootstrap (Current) ✅
- [x] Project structure
- [x] Manifest parser
- [x] Basic CLI
- [x] Type definitions

### Milestone 3: Overview Generator
- [ ] C4 context diagram
- [ ] C4 container diagram (bounded contexts)
- [ ] Per-context detail diagrams
- [ ] Integration events diagram

### Milestone 4: Aggregate Generator
- [ ] Aggregate detail pages
- [ ] Command/event listings
- [ ] Sequence diagrams

### Milestone 5: Flows Generator
- [ ] Cross-aggregate flow detection
- [ ] Saga visualization

## Contributing

This tool is part of the VSA monorepo. See the main VSA documentation for contributing guidelines.

## License

MIT
