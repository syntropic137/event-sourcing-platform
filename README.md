# Event Sourcing Platform

A comprehensive event sourcing platform that packages a robust event store with higher-level event sourcing abstractions. This platform provides reliable, robust, and flexible packages for implementing event sourcing in different applications, with progressive examples serving as living documentation.

[![CI](https://github.com/NeuralEmpowerment/event-sourcing-platform/actions/workflows/ci.yml/badge.svg)](https://github.com/NeuralEmpowerment/event-sourcing-platform/actions/workflows/ci.yml)
[![CodeQL](https://github.com/NeuralEmpowerment/event-sourcing-platform/actions/workflows/codeql.yml/badge.svg)](https://github.com/NeuralEmpowerment/event-sourcing-platform/actions/workflows/codeql.yml)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

## 📋 Table of Contents

- [Architecture Overview](#architecture-overview)
- [Quick Start](#quick-start)
- [Core Components](#core-components)
- [Examples](#examples)
- [Development](#development)
- [Testing](#testing)
- [Contributing](#contributing)

## 🏗️ Architecture Overview

The platform is organized into distinct contexts following Domain-Driven Design principles:

```
event-sourcing-platform/
├── event-store/           # Rust event store service, gRPC server, and client SDKs
│   ├── eventstore-core/       # Shared traits, errors, protobuf types
│   ├── eventstore-backend-*/  # Memory + Postgres backends
│   └── eventstore-bin/        # gRPC server binary
├── event-sourcing/        # Event sourcing SDKs and abstractions
│   ├── rust/                  # Rust SDK (alpha)
│   ├── typescript/            # TypeScript SDK (stable)
│   └── python/                # Python SDK (beta)
├── vsa/                   # Vertical Slice Architecture Manager (beta)
│   ├── vsa-core/             # Core Rust library
│   ├── vsa-cli/              # CLI tool
│   └── vsa-wasm/             # WASM bindings for Node.js
├── examples/              # TypeScript "living documentation" examples
│   ├── 001-basic-store-ts/    # Direct event store usage
│   ├── 002-simple-aggregate-ts/
│   ├── …
│   └── 009-web-dashboard-ts/
├── dev-tools/             # Local Postgres/Redis helper scripts and Docker Compose
├── infra-as-code/         # Terraform + Ansible scaffolding (work in progress)
├── docs-site/             # Docusaurus documentation site (work in progress)
└── docs/                  # Project notes and supporting docs
```

## 🚀 Quick Start

### Prerequisites

- **Rust** (latest stable) - for the event store
- **Node.js** (18+) - for TypeScript SDK and examples
- **Python** (3.8+) - for Python SDK and examples
- **Docker** - for development services
- **PostgreSQL** - for persistent storage (via Docker)

### Setup

1. **Clone and enter the repository:**
   ```bash
   git clone https://github.com/<org>/event-sourcing-platform.git
   cd event-sourcing-platform
   ```

2. **Install workspace dependencies (pnpm 9+ recommended):**
   ```bash
   pnpm -w install
   ```

3. **Start local infrastructure (PostgreSQL + Redis) – optional:**
   ```bash
   make dev-start      # or `make start-services` for the lightweight compose stack
   ```

4. **Build and smoke-test the platform:**
   ```bash
   make build
   make smoke-test     # runs the Rust event-store smoke check
   ```

### Try the Examples

Start with the progressive learning examples:

```bash
# Basic event store usage (no event sourcing)
make examples-001

# Simple event sourcing with one aggregate
make examples-002

# Multiple aggregates working together
make examples-003
```

## 🔧 Core Components

### Event Store Context

**Purpose:** Pure event storage and retrieval  
**Dependencies:** None (standalone)  
**Technology:** Rust with gRPC API

The event store provides:
- ✅ Durable event storage with optimistic concurrency
- ✅ Client-proposed sequence numbers (true optimistic concurrency)
- ✅ Multiple backends (memory, PostgreSQL)
- ✅ gRPC API with protocol buffer definitions
- ✅ Basic client libraries for multiple languages

```bash
cd event-store
make help
```

### Event Sourcing Context

**Purpose:** Event sourcing patterns and abstractions  
**Dependencies:** Event Store context  
**Technology:** Multi-language SDKs (Rust, TypeScript, Python)

The event sourcing SDKs provide:
- 🔄 Aggregate abstractions and lifecycle management
- 🔄 Command/Event handling patterns
- 🔄 Repository pattern implementations
- 🔄 Projection and read model management
- 🔄 Rich developer experience with type safety

```bash
cd event-sourcing
make help
```

### Examples Context

**Purpose:** Living documentation and progressive learning  
**Dependencies:** Event Store + Event Sourcing contexts  
**Technology:** Real applications with no mocks

The examples demonstrate:
- 📚 Progressive learning from basics to complete systems
- 📚 Real working code with actual databases
- 📚 Docker Compose setups for easy experimentation
- 📚 Best practices and patterns
- 📚 Performance considerations

```bash
cd examples
make help
```

## 📚 Examples

All examples are implemented in TypeScript today. They default to the gRPC event store provided by `dev-tools`; append `-- --memory` to run against the in-memory client.

| Example | Status | Highlights |
| ------- | ------ | ---------- |
| 001-basic-store-ts | ✅ Ready | Append/read streams, optimistic concurrency basics |
| 002-simple-aggregate-ts | ✅ Ready | Aggregate decorators, repository pattern |
| 003-multiple-aggregates-ts | ✅ Ready | Aggregate collaboration and sequencing |
| 004-cqrs-patterns-ts | ✅ Ready | Separate write/read models with projections |
| 005-projections-ts | ✅ Ready | Analytics projections and reporting views |
| 006-event-bus-ts | ✅ Ready | Event-driven interactions across bounded contexts |
| 007-ecommerce-complete-ts | 🚧 Placeholder | Wiring for future full e-commerce workflow |
| 007-inventory-complete-ts | ✅ Ready | Inventory lifecycle with projections and alerts |
| 008-observability-ts | ✅ Ready | Operational metrics and health instrumentation |
| 008-banking-complete-ts | 🚧 Placeholder | Scaffold for banking domain (commands TBD) |
| 009-web-dashboard-ts | ✅ Ready | Express-based dashboard consuming projections |

Run an example with:

```bash
make examples-001           # replace with desired example number
# or directly with pnpm:
pnpm --filter ./examples/001-basic-store-ts run start -- --memory
```

## 🛠️ Development

### Build Commands

```bash
# Build everything
make build

# Build specific components
make event-store
make event-sourcing
make examples
make tools
```

### Testing

```bash
# Run all tests
make test

# Test specific components
make test-event-store
make test-event-sourcing
make test-examples
```

### Quality Assurance

```bash
# Run QA checks on everything
make qa

# QA specific components
make qa-event-store
make qa-event-sourcing
make qa-examples
```

### Service Management

```bash
# Start development services (PostgreSQL, etc.)
make start-services

# Stop development services
make stop-services

# Run smoke tests against services
make smoke-test
```

## 🧪 Testing

Current automated coverage focuses on the pieces that ship today:

- **Rust event store** – cargo unit + integration tests cover the core traits, in-memory backend, Postgres backend, and gRPC server wiring. Postgres tests automatically spin up Testcontainers when a local database is not available.
- **TypeScript event-sourcing SDK** – Jest tests validate aggregate lifecycle, optimistic concurrency, and event serialisation helpers.

Planned additions (not yet automated):

- Cross-language SDK compatibility suites (Rust ↔︎ TypeScript ↔︎ future Python).
- Example-level end-to-end checks and smoke tests for each scenario.
- Performance/stress benchmarks and observability regression tests.

## 🎯 Core Principles

1. **Domain Focus**: Event Store and Event Sourcing define the rules of the event sourcing domain
2. **Living Documentation**: Examples demonstrate how to use core packages to build real applications
3. **Progressive Learning**: Examples build from basic concepts to complete systems
4. **No Mocks**: All examples use real working code with actual databases
5. **Future Extensibility**: Architecture supports future event modeling and code generation

## 🔮 Future Plans

### Event Modeling Package
- Code generation from event models
- Domain-specific language for event modeling
- Multi-language code generation
- Scaffolding and boilerplate reduction

### Additional Backends
- KurrentDB backend
- Kafka backend
- EventStoreDB adapter
- Custom backend implementations

### Enhanced Tools
- Advanced CLI features
- Web UI enhancements
- Code generation tools
- Event modeling tools

## 📖 Documentation

Each component has comprehensive documentation:

- **[Event Store Documentation](event-store/README.md)** - Core event storage
- **[Event Sourcing Documentation](event-sourcing/README.md)** - ES patterns and SDKs
- **[Examples Documentation](examples/README.md)** - Learning examples
- **[Tools Documentation](tools/README.md)** - Development tools

## 🤝 Contributing

We welcome contributions! Please see our contributing guidelines and code of conduct.

### Getting Started
1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests
5. Run `make qa` to ensure quality
6. Submit a pull request

### Development Workflow
1. Run `make setup` for initial setup
2. Use `make dev-setup` for development environment
3. Use `make build` and `make test` during development
4. Use `make qa` before submitting changes

## 📄 License

This project is licensed under the MIT License - see the LICENSE file for details.

## 🏷️ Status

- ✅ **Event Store (Rust)** – Memory and Postgres backends with a production-ready gRPC surface.
- ✅ **TypeScript SDK** – Drives all current examples; adding richer patterns iteratively.
- 🔄 **Rust SDK** – Early alpha; core abstractions present, feature parity in progress.
- 📋 **Python SDK** – Placeholder directory waiting for implementation.
- 🔄 **VSA Tool** – Vertical Slice Architecture Manager in planning phase. See [vsa/README.md](vsa/README.md) for details.
- ✅ **Examples** – TypeScript examples 001–006, 007 inventory, 008 observability, and 009 dashboard are runnable today.
- 🚧 **Examples (future)** – 007 e-commerce and 008 banking are scaffolds awaiting domain logic.
- 🚧 **Infra-as-code & docs-site** – Module scaffolding exists; provider-specific stacks and walkthroughs are being built.

## 📚 Inspiration & References

This platform draws inspiration from and builds upon the work of leading event sourcing practitioners:

- **[Understanding Event Sourcing](https://leanpub.com/eventsourcing)** by Martin Dilger - A comprehensive book combining Event Modeling and Event Sourcing to plan and build software systems of any size and complexity. [Sample code on GitHub](https://github.com/dilgerma/eventsourcing-book).
- **Event Modeling** - [The original Event Modeling article](https://eventmodeling.org/) provides foundational concepts for our approach.

---

**This platform serves as a comprehensive foundation for event sourcing applications, providing both the low-level event storage and high-level patterns needed to build robust, scalable systems.**
