# Event Sourcing Platform

A comprehensive event sourcing platform that packages a robust event store with higher-level event sourcing abstractions. This platform provides reliable, robust, and flexible packages for implementing event sourcing in different applications, with progressive examples serving as living documentation.

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
006-event-sourcing-platform/
├── event-store/           # Core Domain: Event Storage & Retrieval
│   ├── eventstore-core/       # Core traits and types
│   ├── eventstore-proto/      # gRPC protocol definitions
│   ├── eventstore-backend-*/  # Storage backends (memory, postgres)
│   ├── eventstore-bin/        # gRPC server binary
│   └── sdks/                  # Basic client libraries
├── event-sourcing/        # Core Domain: Event Sourcing Patterns
│   ├── rust/                  # Rust SDK with ES patterns
│   ├── typescript/            # TypeScript SDK with ES patterns
│   └── python/                # Python SDK with ES patterns
├── examples/              # Living Documentation & Learning Examples
│   ├── 001-basic-store/       # Simple event store usage
│   ├── 002-simple-aggregate/  # Basic event sourcing
│   ├── 003-multiple-aggregates/ # Multiple aggregates
│   ├── 004-cqrs-patterns/     # Command/Query separation
│   ├── 005-projections/       # Read model projections
│   ├── 006-event-bus/         # Cross-aggregate communication
│   ├── 007-ecommerce-complete/ # Complete e-commerce system
│   ├── 008-banking-complete/  # Complete banking system
│   └── 009-inventory-complete/ # Complete inventory system
├── tools/                 # Development Tools & Future Code Generation
│   ├── cli/                   # CLI tools for management
│   ├── web-ui/               # Web interface for browsing
│   └── helpers/              # Utility libraries
└── tests/                 # Integration Tests
```

## 🚀 Quick Start

### Prerequisites

- **Rust** (latest stable) - for the event store
- **Node.js** (18+) - for TypeScript SDK and examples
- **Python** (3.8+) - for Python SDK and examples
- **Docker** - for development services
- **PostgreSQL** - for persistent storage (via Docker)

### Setup

1. **Clone and setup:**
   ```bash
   cd experiments/006-event-sourcing-platform
   make setup
   ```

2. **Start development services:**
   ```bash
   make start-services
   ```

3. **Build all components:**
   ```bash
   make build
   ```

4. **Run smoke tests:**
   ```bash
   make smoke-test
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

The examples are designed for progressive learning:

### Basic Examples (001-003)
- **001-basic-store**: Direct event store usage without event sourcing
- **002-simple-aggregate**: Single aggregate with command/event flow
- **003-multiple-aggregates**: Multiple aggregates with interactions

### Advanced Examples (004-006)
- **004-cqrs-patterns**: Command/Query separation with read models
- **005-projections**: Building projections and managing read models
- **006-event-bus**: Cross-aggregate communication patterns

### Complete Systems (007-009)
- **007-ecommerce-complete**: Full e-commerce system (Orders, Products, Customers)
- **008-banking-complete**: Full banking system (Accounts, Transfers, Audit)
- **009-inventory-complete**: Full inventory system (Products, Warehouses, Supply Chain)

Each example includes:
- 📖 Comprehensive README with learning objectives
- 🐳 Docker Compose setup for dependencies
- 🧪 Tests demonstrating functionality
- 📊 Performance benchmarks where applicable

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

The platform uses a comprehensive testing strategy:

### Unit Tests
- Event store core functionality
- Event sourcing abstractions
- SDK implementations
- Utility functions

### Integration Tests
- Event store with different backends
- Event sourcing patterns end-to-end
- Cross-language SDK compatibility
- Example validation

### End-to-End Tests
- Complete example workflows
- Docker Compose stack validation
- Performance benchmarks
- Stress testing

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

- ✅ **Event Store**: Complete with memory and PostgreSQL backends
- 🚧 **Event Sourcing SDKs**: In progress
- 🚧 **Examples**: In progress (001 basic TypeScript example present)
- 📋 **Tools**: Planned

---

**This platform serves as a comprehensive foundation for event sourcing applications, providing both the low-level event storage and high-level patterns needed to build robust, scalable systems.**
