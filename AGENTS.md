# AGENTS.md

This is the canonical agent instruction file for the event sourcing platform. It is designed to work across multiple AI agent systems (Claude Code, Cursor, Windsurf, etc.).

## Repository Overview

A comprehensive event sourcing platform organized around Domain-Driven Design principles. Provides a low-level event store (Rust) and high-level event sourcing abstractions (multi-language SDKs), plus a Vertical Slice Architecture (VSA) manager tool for code organization.

**Key Philosophy Documents:**
- [Platform Philosophy](docs/PLATFORM-PHILOSOPHY.md) — What this platform IS and IS NOT
- [Maintainability Doctrine](docs/MAINTAINABILITY-DOCTRINE.md) — Engineering principles for sustainability
- [ADR Index](docs/adrs/ADR-INDEX.md) — Architectural decisions

**Requires:** Rust stable, pnpm 10+, protoc 27+, Docker

## Key Architecture Principles

1. **Domain Focus**: Event Store and Event Sourcing define the rules of the event sourcing domain
2. **Living Documentation**: Examples demonstrate real applications with actual databases (no mocks)
3. **Progressive Learning**: Examples build from basic concepts to complete systems
4. **Multi-Language**: Rust for performance-critical components, TypeScript as primary SDK, Python planned
5. **Bounded Contexts**: VSA tool enforces vertical slice architecture with bounded contexts

## Project Structure

```
event-sourcing-platform/
├── event-store/                 # Rust event store with gRPC API
│   ├── eventstore-core/            # Traits, errors, protobuf types
│   ├── eventstore-proto/           # Protobuf definitions
│   ├── eventstore-backend-memory/  # In-memory backend
│   ├── eventstore-backend-postgres/# Postgres backend
│   ├── eventstore-bin/             # gRPC server binary
│   └── sdks/                       # Client SDKs
│       ├── sdk-ts/                    # TypeScript SDK
│       ├── sdk-py/                    # Python SDK (stub)
│       └── sdk-rs/                    # Rust SDK
├── event-sourcing/              # Event sourcing SDKs and patterns
│   ├── typescript/                 # Primary SDK with decorators
│   ├── rust/                       # Alpha SDK
│   └── python/                     # Python SDK (early stage)
├── vsa/                         # Vertical Slice Architecture Manager
│   ├── vsa-core/                   # Core validation logic (Rust)
│   ├── vsa-cli/                    # CLI tool
│   ├── vsa-wasm/                   # WASM bindings for Node.js
│   ├── vsa-visualizer/             # Visualization tooling
│   └── vscode-extension/           # VS Code integration
├── examples/                    # TypeScript "living documentation"
│   ├── 002-simple-aggregate-ts/    # Aggregate decorators
│   ├── 004-cqrs-patterns-ts/      # CQRS patterns
│   └── 007-ecommerce-complete-ts/  # Complete e-commerce example
├── dev-tools/                   # Development infrastructure scripts
├── docs/                        # Project documentation
├── docs-site/                   # Docusaurus documentation site
├── reference/                   # Reference materials
└── infra-as-code/               # Infrastructure as Code
    ├── aws/                        # AWS deployment
    ├── proxmox/                    # Proxmox deployment
    └── shared/                     # Shared provisioning
```

## Where to Look

- **Event store changes** → `event-store/`, proto in `eventstore-proto/proto/`, regenerate stubs with `make gen-ts`/`gen-py`
- **TypeScript SDK** → `event-sourcing/typescript/`
- **Examples** → `examples/`, follow existing patterns and numbering
- **VSA tool** → `vsa/`, Rust workspace with `cargo build --workspace`
- **CI/CD** → `.github/workflows/`
- **Dev infrastructure** → `dev-tools/dev` script, controlled via `make dev-*`
- **Docs site** → `docs-site/` (Docusaurus)

## Essential Commands

### Building

```bash
make build                # Build everything (Rust → Python → TypeScript)
make build-rust           # Event store + Rust SDKs + VSA
make build-typescript     # All TypeScript packages (via Turborepo)
make build-python         # Python SDKs (via uv)
```

### Testing

```bash
make test                 # Run all tests
make test-event-store     # Component-specific
make test-event-sourcing
make test-examples
make test-fast            # Uses dev infrastructure, no testcontainers
```

### Quality Assurance

```bash
make qa                   # Fast QA (static checks + unit tests, no coverage)
make qa-full              # Full QA (includes integration tests + coverage)
make qa-event-store       # Auto-detects dev infrastructure
make qa-event-sourcing
make qa-examples
```

- `make qa` skips slow tests and coverage — use for pre-commit checks
- `make qa-full` runs the complete suite including integration tests and coverage
- **Full end-to-end QA** requires dev infrastructure running (`make dev-init && make dev-start` first). Without it, event store tests fall back to testcontainers (slower, less realistic). Run `make qa-full` with dev infrastructure before releases and version bumps to exercise the real Postgres backend.

### Development Infrastructure

```bash
make dev-init             # Initialize (first time only)
make dev-start            # Start Postgres + Redis
make dev-stop             # Stop infrastructure
make dev-status           # Check what's running
make dev-clean            # Remove all containers and data
```

### Running Examples

```bash
make examples-002         # Simple aggregate (auto-builds dependencies)
make examples-004         # CQRS patterns
make examples-007         # E-commerce complete
```

### Event Store Server

```bash
cd event-store && make run                    # Memory backend (default)
cd event-store && make run BACKEND=postgres   # Postgres backend
cd event-store && make smoke                  # Smoke test
```

### Docs

```bash
make docs                 # Start Docusaurus dev server
make docs-build           # Build static site
```

## Key Concepts

### Event Store Architecture

The Rust event store is the foundation:
- **Backend-agnostic**: Traits in `eventstore-core/` allow pluggable backends
- **Optimistic Concurrency**: Client-proposed sequence numbers (true OCC)
- **gRPC API**: Defined in `eventstore-proto/proto/`
- **Multiple Backends**: Memory (dev/test), Postgres (production)

### Event Sourcing SDK Patterns (TypeScript)

The TypeScript SDK (`event-sourcing/typescript/`) provides:

1. **AggregateRoot**: Base class for aggregates with automatic event replay
2. **@CommandHandler**: Decorator for business logic/validation methods that emit events
3. **@EventSourcingHandler**: Decorator for state mutation methods (replays events)
4. **Repository Pattern**: `RepositoryFactory` creates repositories with OCC tracking
5. **Concurrency Control**: `ConcurrencyConflictError` on stale aggregate saves
6. **Event Bus**: Cross-context communication via integration events

### Vertical Slice Architecture (VSA)

The VSA tool enforces:
- **Vertical Slices**: Each feature is self-contained (command, event, aggregate, tests)
- **Bounded Contexts**: Explicit boundaries via `vsa.yaml`
- **Integration Events**: Single source of truth in `_shared/integration-events/`
- **No Cross-Context Imports**: Contexts communicate only via integration events
- **Commands as Classes**: Commands must be classes with `aggregateId` property
- **Aggregates Handle Commands**: Use `@CommandHandler` on aggregate methods (no separate handler classes)

## Agent Workflow Guidelines

### Development Process

1. **Understand** — Read relevant files and understand context before making changes
2. **Plan** — For non-trivial changes, outline the approach before implementing
3. **Implement** — Make changes following existing patterns and conventions
4. **Verify** — Run `make qa` after changes
5. **Commit** — Use conventional commit format

### QA Checkpoint Process

After completing a logical unit of work:
1. Run linter with auto-formatting
2. Run type checks
3. Run tests
4. Review changes
5. Commit with conventional commit messages

### Conventional Commit Format

Format: `type(scope): description`

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`

### Architecture Decision Records

Significant architecture decisions should be captured in an ADR in `/docs/adrs/`.

### Test-Driven Development

Add tests first, then implement features. Testing code is as important as production code.

### Code Annotations

- `TODO` — things that can be improved or changed in the future
- `FIXME` — things that are breaking the app

### Files to NEVER Commit (Scratch Pads)
- **PROJECT-PLAN_*.md** — Planning documents (gitignored)
- **DEPENDENCY-AUDIT.md** — Analysis scratch pads
- **BACKWARDS-COMPATIBILITY-*.md** — Analysis documents
- ***-ANALYSIS.md** — Any analysis documents
- **PLAN-SUMMARY.md** — Planning summaries
- Temporary files, build artifacts, cache files

**Rule:** Only commit official documentation that belongs in the repository permanently. Scratch pads and planning documents stay local only!

## Testing Philosophy

1. **No Mocks**: All examples use real infrastructure (event store + Postgres/Redis)
2. **Fast Tests**: Leverage dev infrastructure instead of testcontainers when available
3. **Integration Over Unit**: Focus on end-to-end scenarios
4. **Coverage Targets**: Event store maintains 75%+ coverage (aiming for 85-90%)

## Important Files

- **`Makefile`** (root): Orchestrates all builds and tests
- **`turbo.json`**: Turborepo build pipeline configuration
- **`event-store/Makefile`**: Event store commands with dev infrastructure integration
- **`dev-tools/dev`**: Development infrastructure management script
