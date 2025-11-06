# Rust Event Sourcing Examples ✅ ADR-004 COMPLIANT

This directory contains Rust examples demonstrating event sourcing patterns with the event-sourcing-rust SDK.

## Examples Overview

### 1. basic_aggregate.rs ⭐ Beginner
**Complexity:** Simple user management  
**Focus:** Basic aggregate pattern with command handlers

Demonstrates:
- `Aggregate` trait for state management
- `AggregateRoot` trait for command handling
- Business validation in `handle_command()`
- State updates only in `apply_event()`
- Commands as enums
- Error handling with `Result<Vec<Event>>`

**Commands:** 5 (CreateUser, ChangeName, ChangeEmail, Activate, Deactivate)

### 2. order_processing.rs ⭐⭐ Intermediate
**Complexity:** E-commerce order processing  
**Focus:** Complete order lifecycle with state machines

Demonstrates:
- Complex aggregate with multiple states
- Order state machine (Draft → Confirmed → Shipped → Delivered)
- Business rule validation (e.g., can't add items after confirmation)
- Multi-step workflows
- Aggregate helper methods
- Comprehensive error handling

**Commands:** 7 (CreateOrder, AddItem, RemoveItem, ConfirmOrder, ShipOrder, DeliverOrder, CancelOrder)

## ADR-004 Pattern in Rust

Unlike TypeScript and Python which use decorators, Rust implements ADR-004 using traits:

```rust
use async_trait::async_trait;
use event_sourcing_rust::prelude::*;

#[derive(Debug, Clone, Default)]
struct MyAggregate {
    id: Option<String>,
    // ... state fields
    version: u64,
}

// State management trait
impl Aggregate for MyAggregate {
    type Event = MyEvent;
    type Error = Error;

    fn aggregate_id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    fn version(&self) -> u64 {
        self.version
    }

    // EVENT SOURCING - State updates only
    fn apply_event(&mut self, event: &Self::Event) -> Result<()> {
        // Update state based on event
        // NO validation, NO business logic
        Ok(())
    }
}

// Command handling trait
#[async_trait]
impl AggregateRoot for MyAggregate {
    type Command = MyCommand;

    // COMMAND HANDLER - Business logic and validation
    async fn handle_command(&self, command: Self::Command) 
        -> Result<Vec<Self::Event>> 
    {
        // 1. Validate business rules
        // 2. Return events to apply
        Ok(vec![/* events */])
    }
}
```

### Key Differences from TypeScript/Python

| Aspect | TypeScript/Python | Rust |
|--------|------------------|------|
| Aggregate Marking | `@Aggregate` decorator | `impl Aggregate` trait |
| Command Handlers | `@CommandHandler` decorator | `impl AggregateRoot` trait |
| Event Handlers | `@EventSourcingHandler` decorator | `apply_event()` method |
| Event Emission | `this.apply(event)` / `self._apply(event)` | Return `Vec<Event>` |
| Async Support | Native | `#[async_trait]` macro |
| Type Safety | Runtime types | Compile-time guarantees |

### Universal ADR-004 Principles

All three language implementations follow these principles:

1. ✅ **Command handlers integrated in aggregates** (not separate classes)
2. ✅ **Business validation in command handlers**
3. ✅ **State updates only in event sourcing handlers**
4. ✅ **Events are immutable facts**
5. ✅ **Commands validated before events are applied**
6. ✅ **Aggregates enforce business invariants**

## Running the Examples

### Prerequisites

- Rust 1.70+ (stable)
- Cargo

### Build Examples

```bash
# Build all examples
cd event-sourcing/rust
cargo build --examples

# Build specific example
cargo build --example basic_aggregate
cargo build --example order_processing
```

### Run Examples

```bash
# Run basic aggregate
cargo run --example basic_aggregate

# Run order processing
cargo run --example order_processing
```

Expected output shows:
- Commands being dispatched
- Business validation working
- Events being applied
- State updates
- Error handling for invalid operations

### Run Tests

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture
```

## Example Output

### basic_aggregate.rs

```
👤 Basic Aggregate Example - ADR-004 Compliant
===============================================

📝 Step 1: Create User
   ✓ User created: user-123
   Name: John Doe
   Email: john@example.com

✅ Step 2: Activate User
   ✓ User activated

📝 Step 3: Change Name
   ✓ Name changed to: John Smith

📧 Step 4: Change Email
   ✓ Email changed to: john.smith@example.com

📊 Final User Summary:
   User ID: user-123
   Name: John Smith
   Email: john.smith@example.com
   Active: true

🔒 Demonstrating Business Rule Validation:
   Attempting to activate already active user...
   ✓ Correctly rejected: InvalidCommand { message: "User is already active" }

✅ ADR-004 Pattern Demonstrated:
   • Commands validated in handle_command()
   • Events applied in apply_event()
   • Business rules enforced
   • Invalid operations prevented
```

### order_processing.rs

```
🛒 Order Processing Example - ADR-004 Compliant
================================================

📦 Step 1: Create Order
   ✓ Order created: order-123

📝 Step 2: Add Items
   ✓ Added 2x product-1 @ $29.99
   ✓ Added 1x product-2 @ $49.99
   Total: $109.97

✅ Step 3: Confirm Order
   ✓ Order confirmed (Status: Confirmed)

📮 Step 4: Ship Order
   ✓ Order shipped (Tracking: TRK123456789)

🎉 Step 5: Deliver Order
   ✓ Order delivered!

📊 Final Order Summary:
   Order ID: order-123
   Customer: customer-456
   Status: Delivered
   Items: 2
   Total: $109.97

🔒 Demonstrating Business Rule Validation:
   Attempting to add item to delivered order...
   ✓ Correctly rejected: InvalidCommand { message: "Cannot add items to confirmed order" }

✅ ADR-004 Pattern Demonstrated:
   • Commands validated in handle_command()
   • Events applied in apply_event()
   • Business rules enforced
   • Invalid operations prevented
```

## Code Structure

Each example follows this structure:

```
example_name.rs
├── Aggregate struct      # State container
├── Event enum           # Domain events
├── Command enum         # Commands
├── impl Aggregate       # State management (apply_event)
├── impl AggregateRoot   # Command handling (handle_command)
└── main()              # Demo flow
```

## Best Practices

### 1. Validation in Command Handlers

```rust
async fn handle_command(&self, command: Self::Command) -> Result<Vec<Self::Event>> {
    match command {
        MyCommand::DoSomething { value } => {
            // ✅ DO: Validate business rules
            if self.id.is_none() {
                return Err(Error::invalid_command("Aggregate does not exist"));
            }
            if value < 0 {
                return Err(Error::invalid_command("Value must be positive"));
            }
            
            // ✅ DO: Return events
            Ok(vec![MyEvent::SomethingDone { value }])
        }
    }
}
```

### 2. State Updates in Event Handlers

```rust
fn apply_event(&mut self, event: &Self::Event) -> Result<()> {
    match event {
        MyEvent::SomethingDone { value } => {
            // ✅ DO: Only update state
            self.value = *value;
            self.version += 1;
            
            // ❌ DON'T: Validate or throw errors
            // ❌ DON'T: Make decisions
            // ❌ DON'T: Call external services
        }
    }
    Ok(())
}
```

### 3. Error Handling

```rust
// Use the Error constructors provided by the SDK
Error::invalid_command("Reason")
Error::invalid_state("Reason")
Error::domain("Reason")
```

### 4. Commands as Enums

```rust
#[derive(Debug, Clone)]
enum MyCommand {
    // Include aggregate ID in creation command
    Create { id: String, data: String },
    
    // No ID needed for subsequent commands
    // (aggregate already loaded)
    Update { data: String },
}

impl Command for MyCommand {}
```

## Learning Path

1. **Start with basic_aggregate.rs**
   - Learn the `Aggregate` trait
   - Learn the `AggregateRoot` trait
   - Understand command/event separation

2. **Move to order_processing.rs**
   - See complex workflows
   - Learn state machines
   - Understand multi-step processes

3. **Explore the SDK source**
   - `event-sourcing-rust/src/aggregate.rs`
   - `event-sourcing-rust/src/command.rs`
   - `event-sourcing-rust/src/error.rs`

## Related Documentation

- [ADR-004: Command Handlers in Aggregates](../../../docs/adrs/ADR-004-command-handlers-in-aggregates.md)
- [Event Sourcing Rust SDK](../README.md)
- [Multi-Language Pattern Comparison](../../../docs/adrs/ADR-004-command-handlers-in-aggregates.md#language-specific-implementations)

## Common Issues

### async_trait Required

If you see errors about `async fn in traits`, make sure you have:

```toml
[dependencies]
async-trait = "0.1"
```

And use:

```rust
use async_trait::async_trait;

#[async_trait]
impl AggregateRoot for MyAggregate {
    // ...
}
```

### tokio Runtime

The examples use `#[tokio::main]` for async support:

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
```

## Contributing

Found an issue or want to improve an example?

1. Follow the ADR-004 pattern
2. Add comprehensive validation
3. Include error handling demonstrations
4. Add helpful console output
5. Submit a pull request

## License

MIT

