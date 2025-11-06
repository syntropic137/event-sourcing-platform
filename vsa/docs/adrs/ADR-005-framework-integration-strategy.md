# ADR-005: Framework Integration Strategy

**Status:** Accepted  
**Date:** 2025-11-05  
**Deciders:** Architecture Team  
**Context:** How VSA integrates with event sourcing frameworks

## Context and Problem Statement

VSA should work well with the event-sourcing-platform, but also:
- Support vanilla JavaScript/Python/Rust
- Support other event sourcing frameworks
- Not force framework adoption
- Generate idiomatic code for each framework

How do we achieve optional framework integration?

## Decision Drivers

- **Optional**: Must work without any framework
- **Integration**: Should integrate seamlessly with event-sourcing-platform
- **Extensible**: Should support other frameworks
- **Code Generation**: Generated code should use framework base classes
- **Validation**: Should validate framework patterns are followed
- **Multi-Language**: TypeScript, Python, Rust support

## Considered Options

### Option 1: Framework Required
VSA only works with event-sourcing-platform
- ✅ Seamless integration
- ❌ Limited adoption
- ❌ Vendor lock-in

### Option 2: Framework Agnostic
VSA has no framework awareness
- ✅ Maximum flexibility
- ❌ No integration benefits
- ❌ Generated code generic

### Option 3: Optional Framework Integration (Chosen)
VSA works standalone, optionally integrates with frameworks
- ✅ Works without framework
- ✅ Seamless integration when configured
- ✅ Extensible to other frameworks
- ✅ Generated code uses framework base classes

## Decision Outcome

**Chosen option: Option 3 - Optional Framework Integration**

### Configuration

```yaml
vsa:
  version: 1
  root: ./src/contexts
  language: typescript
  
  # Optional: Framework integration
  framework: event-sourcing-platform  # Shorthand for preset
  
  # Or explicit configuration:
  framework:
    name: event-sourcing-platform
    base_types:
      domain_event:
        import: "@event-sourcing-platform/typescript"
        class: "BaseDomainEvent"
      
      aggregate:
        import: "@event-sourcing-platform/typescript"
        class: "AggregateRoot"
      
      event_sourcing_handler:
        import: "@event-sourcing-platform/typescript"
        decorator: "EventSourcingHandler"
      
      repository:
        import: "@event-sourcing-platform/typescript"
        class: "Repository"
```

### Code Generation WITH Framework

```typescript
// Generated: contexts/warehouse/products/create-product/ProductCreatedEvent.ts

import { BaseDomainEvent } from '@event-sourcing-platform/typescript';

export class ProductCreatedEvent extends BaseDomainEvent {
  readonly eventType = 'ProductCreatedEvent' as const;
  readonly schemaVersion = 1 as const;
  
  constructor(
    public readonly productId: string,
    public readonly name: string,
    public readonly sku: string,
  ) {
    super();
  }
}
```

### Code Generation WITHOUT Framework

```typescript
// Generated: contexts/warehouse/products/create-product/ProductCreatedEvent.ts

export class ProductCreatedEvent {
  readonly eventType = 'ProductCreatedEvent' as const;
  readonly schemaVersion = 1 as const;
  
  constructor(
    public readonly productId: string,
    public readonly name: string,
    public readonly sku: string,
  ) {}
}
```

### Template System

Templates use conditional logic based on framework config:

```handlebars
{{! templates/typescript/domain-event.hbs }}

{{#if framework.base_types.domain_event}}
import { {{framework.base_types.domain_event.class}} } from '{{framework.base_types.domain_event.import}}';

export class {{event_name}} extends {{framework.base_types.domain_event.class}} {
{{else}}
export class {{event_name}} {
{{/if}}
  readonly eventType = '{{event_name}}' as const;
  readonly schemaVersion = 1 as const;
  
  constructor(
    {{#each fields}}
    public readonly {{this.name}}: {{this.type}},
    {{/each}}
  ) {
    {{#if framework.base_types.domain_event}}
    super();
    {{/if}}
  }
}
```

### Framework Presets

Built-in presets for common frameworks:

```rust
// vsa-core/src/framework/presets.rs

pub enum FrameworkPreset {
    EventSourcingPlatform,
    NestJsCqrs,
    Axon,
    EventStore,
    Custom(FrameworkConfig),
}

impl FrameworkPreset {
    pub fn event_sourcing_platform() -> FrameworkConfig {
        FrameworkConfig {
            name: "event-sourcing-platform".to_string(),
            base_types: hashmap! {
                "domain_event" => TypeConfig {
                    import: "@event-sourcing-platform/typescript".to_string(),
                    class: Some("BaseDomainEvent".to_string()),
                    interface: None,
                    decorator: None,
                },
                "aggregate" => TypeConfig {
                    import: "@event-sourcing-platform/typescript".to_string(),
                    class: Some("AggregateRoot".to_string()),
                    interface: None,
                    decorator: None,
                },
                "event_sourcing_handler" => TypeConfig {
                    import: "@event-sourcing-platform/typescript".to_string(),
                    class: None,
                    interface: None,
                    decorator: Some("EventSourcingHandler".to_string()),
                },
                // ... more base types
            },
        }
    }
}
```

### Auto-Detection

VSA can detect framework from dependencies:

```bash
vsa init

> Detected @event-sourcing-platform/typescript in package.json
> Use event-sourcing-platform framework? (Y/n): y

✅ Configured framework: event-sourcing-platform
```

### Validation

VSA validates code follows framework patterns:

```bash
vsa validate --framework

✅ ProductCreatedEvent extends BaseDomainEvent
✅ ProductAggregate extends AggregateRoot
✅ Handler uses @EventSourcingHandler decorator

❌ CustomEvent does not extend BaseDomainEvent
   File: contexts/warehouse/custom/CustomEvent.ts
   Expected: export class CustomEvent extends BaseDomainEvent
   
   💡 Fix: vsa fix --add-base-class CustomEvent.ts
```

### Positive Consequences

- **Flexibility**: Works with or without framework
- **Seamless Integration**: Generated code uses event-sourcing-platform
- **Validation**: Ensures framework patterns followed
- **Extensibility**: Easy to add support for other frameworks
- **Auto-Detection**: Minimal configuration needed
- **Multi-Language**: Same approach for TypeScript, Python, Rust

### Negative Consequences

- **Template Complexity**: Templates have conditional logic
- **Maintenance**: Need to maintain framework presets
- **Testing**: Need to test with and without frameworks

## Framework Support Matrix

| Framework | Language | Support | Base Classes |
|-----------|----------|---------|--------------|
| event-sourcing-platform | TypeScript | ✅ Built-in | BaseDomainEvent, AggregateRoot |
| event-sourcing-platform | Python | ✅ Built-in | BaseDomainEvent, Aggregate |
| NestJS CQRS | TypeScript | 🔜 Future | IEvent, AggregateRoot |
| Axon Framework | Java | 🔜 Future | DomainEvent, AggregateRoot |
| EventStoreDB | TypeScript | 🔜 Future | - |
| Vanilla | All | ✅ Built-in | None (no base classes) |
| Custom | All | ✅ Built-in | User-configured |

## CLI Commands

### Initialize with Framework

```bash
# Preset
vsa init --framework event-sourcing-platform

# Auto-detect
vsa init  # Detects from package.json

# Custom
vsa init --framework custom
> Base event class import: @my-framework/core
> Base event class name: BaseEvent
```

### Add Framework to Existing Project

```bash
vsa config set-framework event-sourcing-platform

✅ Updated vsa.yaml
💡 Regenerate features to use framework base classes:
   vsa migrate --add-framework
```

### Validate Framework Usage

```bash
vsa validate --framework

Checking framework integration...

✅ 45 events extend BaseDomainEvent
✅ 12 aggregates extend AggregateRoot
✅ 45 handlers use correct decorators

❌ 3 events don't extend BaseDomainEvent
   Run: vsa fix --add-base-class
```

## Example: event-sourcing-platform Integration

### Full Configuration

```yaml
vsa:
  version: 1
  root: ./src/contexts
  language: typescript
  
  # Event Sourcing Platform integration
  framework: event-sourcing-platform
  
  bounded_contexts:
    - name: warehouse
      path: warehouse/
      publishes: [ProductStockChanged]
      subscribes: [OrderPlaced]
```

### Generated Aggregate

```typescript
// contexts/warehouse/products/create-product/ProductAggregate.ts

import { AggregateRoot } from '@event-sourcing-platform/typescript';
import { EventSourcingHandler } from '@event-sourcing-platform/typescript';
import { ProductCreatedEvent } from './ProductCreatedEvent';

export class ProductAggregate extends AggregateRoot<ProductCreatedEvent> {
  private name: string = '';
  private sku: string = '';
  
  getAggregateType(): string {
    return 'Product';
  }
  
  create(name: string, sku: string): void {
    this.initialize(this.aggregateId);
    this.raiseEvent(new ProductCreatedEvent(this.aggregateId, name, sku));
  }
  
  @EventSourcingHandler('ProductCreatedEvent')
  private onProductCreated(event: ProductCreatedEvent): void {
    this.name = event.name;
    this.sku = event.sku;
  }
}
```

**Result**: Generated code integrates perfectly with event-sourcing-platform infrastructure!

## Migration Strategy

### Adding Framework Later

1. Initialize VSA without framework
2. Build features with vanilla code
3. Later, add framework to config
4. Run migration tool to update code
5. Validate all code updated

```bash
# Step 1: Init without framework
vsa init --no-framework

# Step 2: Build features
vsa generate feature warehouse/products/create-product

# Step 3: Add framework later
vsa config set-framework event-sourcing-platform

# Step 4: Migrate existing code
vsa migrate --add-framework

# Step 5: Validate
vsa validate --framework
```

## Extensibility

Users can define custom frameworks:

```yaml
framework:
  name: my-custom-framework
  base_types:
    domain_event:
      import: "@my-company/events"
      class: "DomainEvent"
    
    aggregate:
      import: "@my-company/aggregates"
      class: "Aggregate"
```

VSA uses this configuration for generation and validation!

## Links

- [Event Sourcing Platform Documentation](../event-sourcing/)
- [NestJS CQRS](https://docs.nestjs.com/recipes/cqrs)
- [Axon Framework](https://docs.axoniq.io/)
- [EventStoreDB](https://www.eventstore.com/)

