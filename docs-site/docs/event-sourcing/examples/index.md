# Examples Overview

The Event Sourcing Platform includes 9 comprehensive examples that demonstrate progressive complexity from basic concepts to advanced patterns.

## 🎯 Learning Path

The examples are designed as a progressive learning journey:

```
001: Basic Store → 002: Simple Aggregate → 003: Multiple Aggregates → 
004: CQRS Patterns → 005: Projections → 006: Event Bus → 
007: Inventory System → 008: Observability → 009: Web Dashboard
```

## 📚 Example Catalog

### Foundation Examples (001-003)
- **001-basic-store-ts** - Raw event store usage (append/read/exists)
- **002-simple-aggregate-ts** - Aggregates with decorators & optimistic concurrency
- **003-multiple-aggregates-ts** - Multiple aggregates working together

### Pattern Examples (004-006)
- **004-cqrs-patterns-ts** - Command/Query separation with read models
- **005-projections-ts** - Event-driven projections and analytics
- **006-event-bus-ts** - Cross-aggregate communication via events

### Complete System Examples (007-009)
- **007-inventory-complete-ts** - Complete inventory management system
- **008-observability-ts** - System monitoring and health metrics
- **009-web-dashboard-ts** - Live HTML dashboard showing projections

## 🚀 Getting Started

### Prerequisites
1. **Development Infrastructure**: Start with `make dev-start`
2. **Event Store Server**: Run the gRPC server (see Event Store docs)
3. **Dependencies**: Install with `pnpm install`

### Running Examples

Each example can run in two modes:

#### Memory Mode (Fast)
```bash
pnpm --filter "./examples/001-basic-store-ts" run start -- --memory
```

#### gRPC Mode (Full Infrastructure)
```bash
# Start infrastructure first
make dev-start

# Start event store server (in separate terminal)
cd event-store
BACKEND=postgres DATABASE_URL=postgres://dev:dev@localhost:15648/dev cargo run -p eventstore-bin

# Run example
pnpm --filter "./examples/001-basic-store-ts" run start
```

## 🎯 Key Learning Objectives

### Core Concepts
- **Event Sourcing Fundamentals**: How events capture state changes
- **Aggregate Design**: Encapsulating business logic and invariants
- **Event Store Operations**: Appending and reading event streams
- **Optimistic Concurrency**: Handling concurrent modifications

### Advanced Patterns
- **CQRS**: Separating command and query responsibilities
- **Projections**: Building read models from event streams
- **Event Bus**: Decoupled cross-aggregate communication
- **Observability**: Monitoring and health checks

### Production Concerns
- **Persistence**: PostgreSQL storage with gRPC interface
- **Error Handling**: Robust error management patterns
- **Testing**: Unit and integration testing strategies
- **Monitoring**: System health and business metrics

## 🔧 Development Workflow

1. **Start with Foundation**: Begin with examples 001-003 to understand basics
2. **Learn Patterns**: Progress through 004-006 for architectural patterns
3. **Study Complete Systems**: Examine 007-009 for real-world implementations
4. **Experiment**: Modify examples to explore different scenarios
5. **Build**: Create your own aggregates and projections

## 📊 Infrastructure Status

The examples demonstrate integration with:
- **PostgreSQL**: Event persistence on port 15648
- **Redis**: Caching and session storage on port 16648
- **gRPC Event Store**: Core event store service on port 50051
- **Web Dashboard**: Live visualization on port 3000

## 🎉 What You'll Build

By completing all examples, you'll have:
- ✅ **Complete understanding** of event sourcing patterns
- ✅ **Production-ready knowledge** of infrastructure setup
- ✅ **Hands-on experience** with TypeScript SDK
- ✅ **Real-world examples** you can adapt for your projects
- ✅ **Live dashboard** showing your event sourcing system in action

Ready to start? Begin with the Basic Store Example (001-basic-store-ts)!
