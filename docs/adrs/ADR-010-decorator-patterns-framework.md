# ADR-010: Decorator Patterns for Framework Integration

**Status:** ✅ Accepted  
**Date:** 2025-11-06  
**Decision Makers:** Architecture Team  
**Related:** Framework Design, Type Safety, Developer Experience

## Context

Our Hexagonal Event-Sourced VSA architecture requires metadata to function:

1. **Event Versioning:** Which version is an event class?
2. **Event Upcasting:** How to migrate between versions?
3. **Command Routing:** Which controller handles which route?
4. **Framework Automation:** Auto-discovery, registration, validation

### The Problem: Metadata Without Decorators

**Option 1: Manual Registration**
```typescript
// Manual and error-prone
EventRegistry.register('ItemAdded', 'v2', ItemAddedEvent);
UpcasterRegistry.register('ItemAdded', 'v1', 'v2', ItemAddedEventUpcaster);
RouteRegistry.register('/api/carts/:id/items', AddItemController.addItem);
```

**Problems:**
- ❌ Easy to forget
- ❌ No compile-time checking
- ❌ Tedious boilerplate
- ❌ Hard to maintain

**Option 2: Configuration Files**
```yaml
# events.yaml
events:
  - name: ItemAdded
    version: v2
    class: ItemAddedEvent
```

**Problems:**
- ❌ Separate from code
- ❌ Gets out of sync
- ❌ No type safety
- ❌ Duplication

### Requirements

1. **Collocated:** Metadata with code
2. **Type-Safe:** Compiler validates
3. **Auto-Discovery:** Framework finds decorated classes
4. **DX-Friendly:** Clean, readable syntax
5. **VSA-Validate:** Can extract metadata for validation

## Decision

We use **decorators** to annotate classes and methods with metadata for framework automation.

### Core Principle

> "Decorators are metadata, not behavior. They describe WHAT something is, not HOW it works."

## Decorator Patterns

### 1. Event Decorator (@Event)

**Purpose:** Mark event classes and declare version

**Signature:**
```typescript
@Event(eventType: string, version: string)
```

**Usage:**
```typescript
import { DomainEvent, Event } from '@event-sourcing-platform/typescript';

@Event('ItemAdded', 'v2')  // eventType, version (REQUIRED)
export class ItemAddedEvent extends DomainEvent {
  constructor(
    public readonly aggregateId: string,
    public readonly productId: string,
    public readonly quantity: number,
    public readonly deviceFingerprint: string  // Added in v2
  ) {
    super();
  }
}
```

**Framework Behavior:**
```typescript
// Framework reads metadata
const eventType = Reflect.getMetadata('event:type', ItemAddedEvent);  // 'ItemAdded'
const version = Reflect.getMetadata('event:version', ItemAddedEvent);  // 'v2'

// Auto-registers in EventRegistry
EventRegistry.register(eventType, version, ItemAddedEvent);

// During deserialization
const event = EventRegistry.deserialize({
  type: 'ItemAdded',
  version: 'v2',
  data: { ...}
});
```

**Rules:**
- ✅ Version parameter REQUIRED
- ✅ Version format: `'v1'`, `'v2'` (primary) or `'1.0.0'` (semantic)
- ✅ EventType matches class name pattern (ItemAdded → ItemAddedEvent)
- ❌ Cannot have duplicate eventType + version combinations

### 2. Upcaster Decorator (@Upcaster)

**Purpose:** Mark upcaster classes and declare version transition

**Signature:**
```typescript
@Upcaster(eventType: string, options: { from: string; to: string })
```

**Usage:**
```typescript
import { EventUpcaster, Upcaster } from '@event-sourcing-platform/typescript';
import { ItemAddedEventV1 } from '../_versioned/ItemAddedEvent.v1';
import { ItemAddedEvent } from '../ItemAddedEvent';

@Upcaster('ItemAdded', { from: 'v1', to: 'v2' })
export class ItemAddedEventUpcasterV1V2 
  implements EventUpcaster<ItemAddedEventV1, ItemAddedEvent> {
  
  upcast(oldEvent: ItemAddedEventV1): ItemAddedEvent {
    return new ItemAddedEvent(
      oldEvent.aggregateId,
      oldEvent.productId,
      oldEvent.quantity,
      'default-fingerprint'  // Default for new field
    );
  }
}
```

**Framework Behavior:**
```typescript
// Auto-registers in UpcasterRegistry
UpcasterRegistry.register('ItemAdded', 'v1', 'v2', ItemAddedEventUpcasterV1V2);

// During deserialization
const event = eventStore.read('ItemAdded', 'v1', data);
const upcaster = UpcasterRegistry.get('ItemAdded', 'v1', 'v2');
const upcastedEvent = upcaster.upcast(event);  // Now v2
```

**Rules:**
- ✅ Must implement `EventUpcaster<From, To>` interface
- ✅ From and To versions must exist
- ✅ One upcaster per version transition
- ❌ Cannot skip versions (must chain: v1→v2→v3)

### 3. REST Controller Decorators

**Purpose:** Define HTTP routes and handlers

**Decorators:**
- `@RestController()` - Mark class as REST controller
- `@Route(path)` - Base route for controller
- `@Get(path)` - HTTP GET handler
- `@Post(path)` - HTTP POST handler
- `@Put(path)` - HTTP PUT handler
- `@Delete(path)` - HTTP DELETE handler
- `@Param(name)` - Route parameter
- `@Body()` - Request body
- `@Query(name)` - Query parameter

**Usage:**
```typescript
import { RestController, Route, Post, Get, Param, Body } from '@vsa/adapters';
import { CommandBus } from '../../infrastructure/CommandBus';
import { QueryBus } from '../../infrastructure/QueryBus';

@RestController()
@Route('/api/carts')
export class CartController {
  constructor(
    private commandBus: CommandBus,
    private queryBus: QueryBus
  ) {}

  @Post('/:cartId/items')
  async addItem(
    @Param('cartId') cartId: string,
    @Body() request: AddItemRequest
  ): Promise<void> {
    await this.commandBus.send(new AddItemCommand(
      cartId,
      request.productId,
      request.quantity
    ));
  }

  @Get('/:cartId/items')
  async getItems(
    @Param('cartId') cartId: string
  ): Promise<CartItemsView> {
    return await this.queryBus.send(new GetCartItemsQuery(cartId));
  }

  @Delete('/:cartId/items/:itemId')
  async removeItem(
    @Param('cartId') cartId: string,
    @Param('itemId') itemId: string
  ): Promise<void> {
    await this.commandBus.send(new RemoveItemCommand(cartId, itemId));
  }
}
```

**Framework Behavior:**
```typescript
// Auto-discovery and registration
const controllers = ControllerScanner.find('@RestController');

controllers.forEach(controller => {
  const basePath = Reflect.getMetadata('route:basePath', controller);
  const methods = Reflect.getMetadata('route:methods', controller.prototype);
  
  methods.forEach(method => {
    const httpMethod = Reflect.getMetadata('route:httpMethod', method);
    const path = Reflect.getMetadata('route:path', method);
    
    router.register(httpMethod, basePath + path, controller[method]);
  });
});

// Result: Automatic route registration
// POST   /api/carts/:cartId/items           → CartController.addItem
// GET    /api/carts/:cartId/items           → CartController.getItems
// DELETE /api/carts/:cartId/items/:itemId   → CartController.removeItem
```

### 4. CLI Controller Decorators

**Purpose:** Define command-line commands

**Decorators:**
- `@CliController()` - Mark class as CLI controller
- `@Command(name)` - CLI command name
- `@Description(text)` - Command description
- `@Argument(name, description)` - Positional argument
- `@Option(name, options)` - Named option

**Usage:**
```typescript
import { CliController, Command, Description, Argument, Option } from '@vsa/adapters';
import { CommandBus } from '../../infrastructure/CommandBus';

@CliController()
export class CartCLI {
  constructor(private commandBus: CommandBus) {}

  @Command('cart:add-item')
  @Description('Add an item to a shopping cart')
  async addItem(
    @Argument('cartId', 'Cart ID') cartId: string,
    @Argument('productId', 'Product ID') productId: string,
    @Option('quantity', { default: 1, description: 'Quantity' }) quantity: number,
    @Option('price', { required: true, description: 'Price' }) price: number
  ): Promise<void> {
    await this.commandBus.send(new AddItemCommand(
      cartId,
      productId,
      quantity,
      price
    ));
    console.log('✅ Item added successfully');
  }

  @Command('cart:list-items')
  @Description('List all items in a cart')
  async listItems(
    @Argument('cartId', 'Cart ID') cartId: string
  ): Promise<void> {
    const result = await this.queryBus.send(new GetCartItemsQuery(cartId));
    console.table(result.items);
  }
}
```

**Framework Behavior:**
```typescript
// Auto-generates CLI help
$ myapp cart --help

Commands:
  cart:add-item <cartId> <productId> [options]
    Add an item to a shopping cart
    
    Arguments:
      cartId      Cart ID
      productId   Product ID
    
    Options:
      --quantity  Quantity (default: 1)
      --price     Price (required)

  cart:list-items <cartId>
    List all items in a cart
    
    Arguments:
      cartId      Cart ID
```

### 5. gRPC Controller Decorators

**Purpose:** Define gRPC service methods

**Decorators:**
- `@GrpcController(serviceName)` - Mark class as gRPC service
- `@GrpcMethod(methodName)` - gRPC method

**Usage:**
```typescript
import { GrpcController, GrpcMethod } from '@vsa/adapters';
import { CommandBus } from '../../infrastructure/CommandBus';

@GrpcController('CartService')
export class CartGrpcService {
  constructor(private commandBus: CommandBus) {}

  @GrpcMethod('AddItem')
  async addItem(request: AddItemGrpcRequest): Promise<void> {
    await this.commandBus.send(new AddItemCommand(
      request.cartId,
      request.productId,
      request.quantity,
      request.price
    ));
  }

  @GrpcMethod('GetItems')
  async getItems(request: GetItemsGrpcRequest): Promise<CartItemsGrpcResponse> {
    const result = await this.queryBus.send(new GetCartItemsQuery(request.cartId));
    return {
      cartId: result.cartId,
      items: result.items
    };
  }
}
```

## Decorator Implementation

### TypeScript Implementation

```typescript
// @Event decorator
export function Event(eventType: string, version: string) {
  return function (constructor: Function) {
    // Store metadata
    Reflect.defineMetadata('event:type', eventType, constructor);
    Reflect.defineMetadata('event:version', version, constructor);
    
    // Auto-register
    EventRegistry.register(eventType, version, constructor as any);
  };
}

// @Upcaster decorator
export function Upcaster(
  eventType: string,
  options: { from: string; to: string }
) {
  return function (constructor: Function) {
    Reflect.defineMetadata('upcaster:eventType', eventType, constructor);
    Reflect.defineMetadata('upcaster:from', options.from, constructor);
    Reflect.defineMetadata('upcaster:to', options.to, constructor);
    
    UpcasterRegistry.register(
      eventType,
      options.from,
      options.to,
      constructor as any
    );
  };
}

// @RestController decorator
export function RestController() {
  return function (constructor: Function) {
    Reflect.defineMetadata('controller:type', 'rest', constructor);
    ControllerRegistry.register('rest', constructor);
  };
}

// @Post decorator
export function Post(path: string) {
  return function (
    target: any,
    propertyKey: string,
    descriptor: PropertyDescriptor
  ) {
    Reflect.defineMetadata('route:httpMethod', 'POST', descriptor.value);
    Reflect.defineMetadata('route:path', path, descriptor.value);
    
    // Store on class prototype
    const routes = Reflect.getMetadata('routes', target.constructor) || [];
    routes.push({ method: 'POST', path, handler: propertyKey });
    Reflect.defineMetadata('routes', routes, target.constructor);
  };
}
```

### Python Implementation

```python
# @event decorator
def event(event_type: str, version: str):
    def decorator(cls):
        # Store metadata
        cls.__event_type__ = event_type
        cls.__event_version__ = version
        
        # Auto-register
        EventRegistry.register(event_type, version, cls)
        return cls
    return decorator

# @upcaster decorator
def upcaster(event_type: str, from_version: str, to_version: str):
    def decorator(cls):
        cls.__upcaster_event_type__ = event_type
        cls.__upcaster_from__ = from_version
        cls.__upcaster_to__ = to_version
        
        UpcasterRegistry.register(event_type, from_version, to_version, cls)
        return cls
    return decorator

# @rest_controller decorator
def rest_controller():
    def decorator(cls):
        cls.__controller_type__ = 'rest'
        ControllerRegistry.register('rest', cls)
        return cls
    return decorator

# @post decorator
def post(path: str):
    def decorator(func):
        func.__http_method__ = 'POST'
        func.__route_path__ = path
        return func
    return decorator
```

### Rust Implementation (Procedural Macros)

```rust
// Procedural macros for Rust
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, ItemFn};

#[proc_macro_attribute]
pub fn event(args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    
    // Parse args: event_type, version
    let (event_type, version) = parse_event_args(args);
    
    let expanded = quote! {
        #input
        
        impl DomainEvent for #name {
            fn event_type(&self) -> &'static str {
                #event_type
            }
            
            fn version(&self) -> &'static str {
                #version
            }
        }
        
        // Auto-register at compile time
        inventory::submit! {
            EventRegistration {
                event_type: #event_type,
                version: #version,
                constructor: #name::from_json
            }
        }
    };
    
    TokenStream::from(expanded)
}

// Usage:
#[event("ItemAdded", "v2")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemAddedEvent {
    pub aggregate_id: String,
    pub product_id: String,
    pub quantity: u32,
}
```

## VSA Framework Integration

### Auto-Discovery

```typescript
// framework/scanner.ts
export class DecoratorScanner {
  static findEvents(directory: string): EventMetadata[] {
    const files = glob.sync(`${directory}/**/*Event.{ts,js}`);
    const events: EventMetadata[] = [];
    
    files.forEach(file => {
      const module = require(file);
      
      Object.values(module).forEach((exported: any) => {
        if (Reflect.hasMetadata('event:type', exported)) {
          events.push({
            class: exported,
            type: Reflect.getMetadata('event:type', exported),
            version: Reflect.getMetadata('event:version', exported),
            file: file
          });
        }
      });
    });
    
    return events;
  }
  
  static findControllers(directory: string): ControllerMetadata[] {
    // Similar pattern for controllers
  }
}
```

### VSA Validation

```bash
vsa validate

# Decorator checks:
# ✓ All events have @Event decorator
# ✓ All events have version parameter
# ✓ All upcasters have @Upcaster decorator
# ✓ All controllers have @RestController/@CliController/@GrpcController
# ✗ ERROR: ItemAddedEvent missing @Event decorator
# ✗ ERROR: ItemAddedEvent missing version parameter
# ✗ ERROR: AddItemController has @RestController but no route methods
```

### OpenAPI Generation

```typescript
// Auto-generate OpenAPI from decorators
export class OpenAPIGenerator {
  generate(): OpenAPISpec {
    const controllers = DecoratorScanner.findControllers('slices');
    const spec: OpenAPISpec = { paths: {} };
    
    controllers.forEach(controller => {
      const basePath = Reflect.getMetadata('route:basePath', controller.class);
      const routes = Reflect.getMetadata('routes', controller.class);
      
      routes.forEach(route => {
        const fullPath = basePath + route.path;
        spec.paths[fullPath] = {
          [route.method.toLowerCase()]: {
            summary: route.description || route.handler,
            parameters: extractParameters(route),
            responses: extractResponses(route)
          }
        };
      });
    });
    
    return spec;
  }
}

// Result: swagger.json auto-generated from decorators
```

## Consequences

### Positive

1. **Clean Syntax** ✅
   - Decorators are concise and readable
   - Metadata colocated with code
   - Self-documenting

2. **Type Safety** ✅
   - TypeScript validates decorator usage
   - Compiler catches errors
   - IDE autocomplete

3. **Auto-Discovery** ✅
   - Framework finds decorated classes
   - No manual registration
   - Less boilerplate

4. **Tool Generation** ✅
   - OpenAPI/Swagger from decorators
   - CLI help from decorators
   - VSA validation from metadata

5. **Consistency** ✅
   - Same pattern across languages (adapted)
   - Predictable behavior
   - Easy to learn

### Negative

1. **Reflection Required** ⚠️
   - TypeScript needs `reflect-metadata`
   - Runtime overhead (minimal)
   - **Mitigation:** Caching, compile-time optimization

2. **Magic Feeling** ⚠️
   - Decorators can feel like magic
   - **Mitigation:** Clear documentation, explicit behavior

3. **Language Limitations** ⚠️
   - Rust uses macros (different syntax)
   - Python decorators work differently
   - **Mitigation:** Adapt to language idioms

### Neutral

1. **Build Configuration**
   - TypeScript needs `experimentalDecorators: true`
   - Rust needs proc-macro dependencies
   - Python works natively

## Related ADRs

- ADR-004: Command Handlers in Aggregates (@CommandHandler, @EventSourcingHandler)
- ADR-005: Hexagonal Architecture for Event-Sourced Systems (decorator context)
- ADR-007: Event Versioning and Upcasters (@Event, @Upcaster)
- ADR-008: Vertical Slices as Hexagonal Adapters (controller decorators)

## References

- TypeScript Decorators Proposal
- Python PEP 318 (Decorators for Functions and Methods)
- Rust Procedural Macros Guide
- NestJS Decorators (inspiration)
- Angular Decorators (inspiration)

---

**Last Updated:** 2025-11-06  
**Supersedes:** None  
**Superseded By:** None

