---
sidebar_position: 1
slug: /vsa/index
---

# Vertical Slice Architecture (VSA) Manager

A powerful CLI tool and VS Code extension for building maintainable, event-sourced applications using Vertical Slice Architecture with Domain-Driven Design principles.

## What is VSA?

**Vertical Slice Architecture** organizes code by business features (vertical slices) rather than technical layers. Each slice contains everything needed for that feature - from API endpoints to domain logic to data persistence.

### Why VSA?

Traditional layered architectures organize code by technical concerns:

```
❌ Layered Architecture
controllers/
  └── OrderController.ts
services/
  └── OrderService.ts
repositories/
  └── OrderRepository.ts
models/
  └── Order.ts
```

**Problems with layers:**
- Changes ripple across multiple directories
- Hard to understand a complete feature
- Difficult for teams to work in parallel
- Creates tight coupling between layers

VSA organizes by business capabilities:

```
✅ Vertical Slice Architecture
contexts/
  └── orders/
      ├── place-order/          # Complete feature!
      │   ├── PlaceOrderCommand.ts
      │   ├── OrderPlacedEvent.ts
      │   ├── PlaceOrderHandler.ts
      │   ├── OrderAggregate.ts
      │   └── PlaceOrder.test.ts
      └── cancel-order/         # Another complete feature!
          ├── CancelOrderCommand.ts
          └── ...
```

**Benefits:**
- ✅ Features are self-contained and easy to understand
- ✅ Teams can work in parallel without conflicts
- ✅ Changes are localized to a single slice
- ✅ Easy to test and maintain
- ✅ Natural alignment with business capabilities

## Key Features

### 🏗️ Convention Over Configuration
- Standard folder structure (`contexts/`, `_shared/`)
- Naming conventions (`*Command.ts`, `*Event.ts`, `*Handler.ts`)
- Automatic validation of architectural rules

### 🎯 Bounded Context Support
- Define explicit context boundaries
- Enforce no direct cross-context imports
- Integration events for context communication
- Single source of truth for shared events

### 🔧 Powerful CLI Tool
- **Scaffolding** - Generate features with proper structure
- **Validation** - Enforce architectural rules automatically
- **Watch Mode** - Real-time validation on file changes
- **Manifest Generation** - Document your architecture

### 💡 VS Code Integration
- Real-time diagnostics and validation
- Quick fixes for common issues
- Command palette integration
- YAML schema auto-completion for `vsa.yaml`

### 🧪 Testing-First Approach
- Test files are first-class citizens
- Unit, integration, and E2E testing support
- Examples include comprehensive test suites

### 🔄 Event Sourcing Integration
- Optional integration with event-sourcing-platform
- CQRS pattern support
- Event store adapter included
- Works with any event sourcing framework

## Quick Example

Generate a complete feature with one command:

```bash
# Initialize VSA
vsa init --language typescript

# Generate a feature
vsa generate orders place-order

# Validate your architecture
vsa validate --watch
```

This creates:

```typescript
// PlaceOrderCommand.ts
export class PlaceOrderCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly customerId: string,
    public readonly items: Array<{ productId: string; quantity: number }>
  ) {}
}

// OrderAggregate.ts
import { Aggregate, AggregateRoot, CommandHandler, EventSourcingHandler } from '@event-sourcing-platform/typescript';

@Aggregate('Order')
export class OrderAggregate extends AggregateRoot {
  private items: Array<{ productId: string; quantity: number }> = [];

  @CommandHandler('PlaceOrderCommand')
  placeOrder(command: PlaceOrderCommand): void {
    // Validate business rules
    if (!command.items || command.items.length === 0) {
      throw new Error('Order must have items');
    }
    
    // Initialize and emit event
    this.initialize(command.aggregateId);
    this.apply(new OrderPlacedEvent(command.aggregateId, command.items));
  }

  @EventSourcingHandler('OrderPlaced')
  private onOrderPlaced(event: OrderPlacedEvent): void {
    this.items = event.items;
  }
}

// PlaceOrder.test.ts
describe('PlaceOrder', () => {
  it('should place an order successfully', () => {
    const aggregate = new OrderAggregate();
    const command = new PlaceOrderCommand('order-1', 'customer-1', [{ productId: 'p1', quantity: 2 }]);
    
    (aggregate as any).handleCommand(command);
    
    expect(aggregate.getUncommittedEvents()).toHaveLength(1);
  });
});
```

## Getting Started

Ready to build with VSA? Follow these guides:

1. **[Installation](./getting-started/installation)** - Install the CLI and VS Code extension
2. **[Quick Start](./getting-started/quick-start)** - Create your first VSA project
3. **[Your First Feature](./getting-started/your-first-feature)** - Generate and implement a feature

## Learn More

### Core Concepts
- [Vertical Slices](./concepts/vertical-slices) - Organizing code by business features
- [Bounded Contexts](./concepts/bounded-contexts) - Domain boundaries with DDD
- [Integration Events](./concepts/integration-events) - Cross-context communication
- [Convention Over Configuration](./concepts/convention-over-configuration) - Standard patterns

### Tools
- [CLI Commands](./cli/commands) - Complete CLI reference
- [VS Code Extension](./ide/vscode-extension) - IDE integration features

### Examples

Check the `vsa/examples/` directory in the repository for complete working examples:
- **Todo List** (`01-todo-list-ts`) - Beginner-friendly introduction
- **Library Management** (`02-library-management-ts`) - Bounded contexts in action

See the repository for more examples and complete source code.

## Why VSA with Event Sourcing?

VSA pairs perfectly with Event Sourcing:

1. **Natural Fit** - Commands and events map directly to slices
2. **Clear Boundaries** - Contexts align with aggregates
3. **Testability** - Event-sourced slices are easy to test
4. **Temporal Queries** - Full audit trail per feature
5. **Scalability** - Independent slices can scale separately

## Project Status

The VSA Manager is in active development and used in production for the event-sourcing platform itself.

- ✅ **CLI Tool** - Stable and feature-complete
- ✅ **VS Code Extension** - Real-time validation working
- ✅ **TypeScript Support** - Fully supported
- ✅ **Examples** - Four complete examples available
- 🚧 **Python Support** - Basic support, being expanded
- 📋 **Rust Support** - Planned

## Community & Support

- **GitHub**: [event-sourcing-platform](https://github.com/neuralempowerment/event-sourcing-platform)
- **Issues**: Report bugs or request features
- **Discussions**: Ask questions and share ideas
- **Examples**: Check the `vsa/examples/` directory

---

**Ready to get started?** Install the VSA Manager and build your first vertical slice!

→ [Installation Guide](./getting-started/installation)


