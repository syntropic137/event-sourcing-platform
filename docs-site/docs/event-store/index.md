# Event Store Documentation Index

Welcome to the comprehensive documentation for the Rust Event Store - a polyglot event store implementation with SDKs for TypeScript, Python, and Rust.

## 📚 Documentation Overview

This documentation is organized by context to help you find what you need quickly:

### 🏗️ Architecture & Design
- **[ADRs (Architectural Decision Records)](adrs/)** - Key design decisions and their rationale
- **[Core Concepts](concepts/)** - Fundamental event sourcing concepts and terminology
- **[Ubiquitous Language](concepts/ubiquitous-language.md)** - Canonical terms and vocabulary

### ⚙️ Technical Implementation
- **[Event Model](concepts/event-model.md)** - Event structure and wire format
- **[Concurrency & Consistency](implementation/concurrency-and-consistency.md)** - Optimistic concurrency patterns
- **[SQL Enforcement](implementation/sql-enforcement.md)** - Database constraints and sequencing
- **[Axon Alignment](implementation/axon-alignment.md)** - Framework compatibility details

### 🛠️ SDKs & APIs
- **[SDK Overview](sdks/overview/sdk-overview.md)** - SDK architecture and workflow
- **[TypeScript SDK](sdks/typescript/typescript-sdk.md)** - Complete TypeScript implementation
- **[Python SDK](sdks/python/python-sdk.md)** - Python async implementation
- **[Rust SDK](sdks/rust/rust-sdk.md)** - Rust native implementation
- **[API Reference](sdks/api-reference.md)** - Complete API documentation

### 🚀 Getting Started

**New to Event Sourcing?**
1. Start with [Ubiquitous Language](concepts/ubiquitous-language.md)
2. Read [ADR 001: Client-Proposed Optimistic Concurrency](adrs/001-client-proposed-optimistic-concurrency.md)
3. Study the [Event Model](concepts/event-model.md)

**Ready to Implement?**
1. Check [Concurrency & Consistency](implementation/concurrency-and-consistency.md)
2. Review [Proto & Clients Setup](implementation/initial-plan_proto-and-clients.md)
3. Study [SDK Design](implementation/sdk-design.md)

## 📖 Quick Reference

| Topic | Location |
|-------|----------|
| Event structure | [Event Model](concepts/event-model.md) |
| Concurrency patterns | [Concurrency & Consistency](implementation/concurrency-and-consistency.md) |
| Database schema | [SQL Enforcement](implementation/sql-enforcement.md) |
| TypeScript client | [TypeScript SDK](sdks/typescript/typescript-sdk.md) |
| API reference | [API Reference](sdks/api-reference.md) |

## 🔗 External Links

- [Project README](../README.md) - Quickstart and setup
- [GitHub Repository](https://github.com/your-repo/event-store) - Source code and issues

---

*This index provides the main entry points to our documentation. For detailed implementation guides and API references, explore the sections above.*
