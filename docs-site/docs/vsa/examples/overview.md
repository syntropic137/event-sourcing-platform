---
sidebar_position: 1
---

# Examples Overview

Learn VSA through hands-on examples with progressive complexity.

## Learning Path

We provide four complete examples, each building on concepts from the previous:

### ⭐ Example 1: Todo List (Beginner)

**Complexity:** Beginner  
**Concepts:** VSA basics, Event Sourcing, CQRS  
**Time:** 1-2 hours

Perfect introduction to VSA:
- Single bounded context
- Basic CQRS pattern
- Event sourcing fundamentals
- Simple aggregates
- Unit and integration tests

→ Todo List Example (`vsa/examples/01-todo-list-ts` in repository)

### ⭐⭐ Example 2: Library Management (Intermediate)

**Complexity:** Intermediate  
**Concepts:** Bounded Contexts, Integration Events, Event Subscribers  
**Time:** 2-3 hours

Real-world multi-context system:
- Multiple bounded contexts (Catalog, Lending, Members)
- Integration events for cross-context communication
- Event subscribers
- Read models and projections
- E2E testing

→ Library Management Example (`vsa/examples/02-library-management-ts` in repository)

### ⭐⭐⭐ Example 3: E-commerce Platform (Advanced)

**Complexity:** Advanced  
**Concepts:** Sagas, Complex Workflows, Production Patterns  
**Time:** 3-4 hours

Production-ready patterns:
- Saga orchestration
- Compensating transactions
- Complex business workflows
- Multiple integration points
- Advanced testing strategies

→ E-commerce Platform Example (coming soon)

### ⭐⭐⭐⭐ Example 4: Banking System (Expert)

**Complexity:** Expert  
**Concepts:** Python, CQRS, Security, Compliance  
**Time:** 4-5 hours

Enterprise-grade system:
- Python implementation
- Advanced CQRS with read models
- Fraud detection
- Security and compliance
- Audit trails

→ Banking System Example (coming soon)

## What You'll Learn

### Beginner Level
- ✅ VSA project structure
- ✅ Commands and events
- ✅ Handlers and aggregates
- ✅ Basic testing
- ✅ Event sourcing fundamentals

### Intermediate Level
- ✅ Bounded contexts
- ✅ Integration events
- ✅ Cross-context communication
- ✅ Event subscribers
- ✅ Read models
- ✅ E2E testing

### Advanced Level
- ✅ Saga patterns
- ✅ Compensating transactions
- ✅ Complex workflows
- ✅ Production deployment
- ✅ Performance optimization

### Expert Level
- ✅ Multi-language support
- ✅ Advanced CQRS
- ✅ Security patterns
- ✅ Compliance requirements
- ✅ Audit and monitoring

## Running the Examples

All examples are located in `vsa/examples/`:

```bash
cd vsa/examples/

# Start infrastructure (Event Store + PostgreSQL)
make start-infra

# Run specific example
cd 01-todo-list-ts
npm install
npm test

# Run all examples
make test-all

# Stop infrastructure
make stop-infra
```

## Example Structure

Each example includes:

```
example/
├── README.md              # Example overview
├── ARCHITECTURE.md        # Architecture decisions
├── package.json
├── src/
│   ├── contexts/         # Bounded contexts
│   ├── _shared/          # Integration events
│   └── infrastructure/   # Event store, buses
├── tests/
│   ├── unit/            # Unit tests
│   ├── integration/     # Integration tests
│   └── e2e/             # End-to-end tests
└── vsa.yaml             # VSA configuration
```

## Next Steps

1. **Start with Example 1** - Build foundation
2. **Progress sequentially** - Each builds on previous
3. **Read the code** - Understand patterns
4. **Run the tests** - See behavior
5. **Experiment** - Modify and learn

---

**Ready to start?** Check out the Todo List example in `vsa/examples/01-todo-list-ts` →

