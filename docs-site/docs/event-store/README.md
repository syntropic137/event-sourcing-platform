# Event Store Documentation

This is the comprehensive documentation for the polyglot event store and SDK implementations. All documentation is organized by context and maintained as the single source of truth.

## 📁 Documentation Structure

### 🎯 Architectural Decisions (adrs/)
Records of key architectural choices and design decisions.

- **ADR 001: Client-Proposed Optimistic Concurrency**
  - Why client proposes aggregateNonce instead of store assignment
  - True optimistic concurrency model with race condition prevention

- **ADR 002: Aggregate vs Stream Terminology**
  - Choice of aggregate-centric terminology over stream-centric
  - DDD alignment and Axon Framework compatibility

### 💡 Core Concepts (concepts/)
Fundamental concepts and domain language of the event store.

- **Ubiquitous Language** - Canonical terms and vocabulary
- **Event Model & Envelope** - Event structure and polyglot contract
- **Aggregate Pattern** - Event-sourced aggregate examples
- **Event Store vs Event Bus** - Architectural separation

### ⚙️ Implementation (implementation/)
Technical specifications and implementation details.

- **Concurrency & Consistency** - Optimistic concurrency flow
- **Axon Alignment** - Framework compatibility details
- **SQL Enforcement** - Database schema and constraints
- **SDK Design** - TypeScript SDK ergonomics
- **Proto & Clients Setup** - Implementation guide

### 🛠️ SDKs
Language-specific SDK implementations and documentation.

- **[SDK Overview](sdks/overview/sdk-overview.md)** - SDK architecture and workflow
- **[TypeScript SDK](sdks/typescript/typescript-sdk.md)** - Complete TypeScript implementation
- **[Python SDK](sdks/python/python-sdk.md)** - Python async implementation
- **[Rust SDK](sdks/rust/rust-sdk.md)** - Rust native implementation
- **[API Reference](sdks/api-reference.md)** - Complete API documentation

### 🔧 Operations (operations/)
Operational concerns and deployment considerations.

*Coming soon: monitoring, configuration, deployment guides*

### 🚀 Development (development/)
Developer resources and external references.

- **Rust Resources** - External Rust event sourcing implementations

## 🎯 Key Architectural Principles

1. **Client-Proposed Optimistic Concurrency** - Client proposes aggregateNonce, store validates
2. **Aggregate-Centric Terminology** - DDD-aligned terms (aggregateId, aggregateNonce, etc.)
3. **Store vs Bus Separation** - Clear separation of durable storage and event distribution
4. **True Optimistic Concurrency** - Race-condition-free sequence management
5. **Polyglot Support** - Consistent API across Rust, TypeScript, Python

## 📖 Getting Started

**New to Event Sourcing?**
1. Start with **Ubiquitous Language** to learn the vocabulary
2. Read **ADR 001: Client-Proposed Optimistic Concurrency** to understand our concurrency model
3. Study **Event Model** for the technical contract
4. Review **Aggregate Pattern** for implementation examples

**Ready to Implement?**
1. Check **Concurrency & Consistency** for optimistic concurrency
2. Review **Proto & Clients Setup** for getting started
3. Study **SDK Design** for client integration

## 🔄 Implementation Status

| Component | Status | Notes |
|-----------|--------|-------|
| **Rust Core** | ✅ Complete | Event store traits and memory backend |
| **gRPC Service** | ✅ Complete | Protocol buffer definitions |
| **TypeScript SDK** | ✅ Complete | Client library with type safety |
| **PostgreSQL Backend** | 🚧 In Progress | Schema and persistence layer |
| **Kurrent DB Backend** | 📋 Planned | EventStoreDB-compatible backend |
| **Python SDK** | 📋 Planned | Basic client implementation |
| **Documentation** | ✅ Complete | Comprehensive docs with ADRs |

## 🎨 Documentation Philosophy

- **Single Source of Truth**: All docs maintained here, docs-site imports from this directory
- **Architectural Decision Records**: Key decisions documented in `adrs/` with rationale
- **Context-Organized**: Documentation structured by usage context (concepts, implementation, etc.)
- **Living Documentation**: Updated with implementation changes and new decisions
- **Developer-First**: Technical depth appropriate for implementation teams

---

**This documentation serves as the authoritative source for all event store architecture, terminology, and implementation details.**
