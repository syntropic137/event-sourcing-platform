# Rust SDK (Coming Soon)

The Rust SDK for Event Sourcing is currently in development.

## 🚧 Status

**Planned Features:**
- Zero-cost abstractions
- Type-safe event handling
- Async/await support with Tokio
- Macro-based event handlers
- Full Rust idioms and patterns
- Integration with Event Store Rust SDK

## 📋 Planned API

```rust
use neurale_event_sourcing::{BaseAggregate, EventHandler, DomainEvent};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OrderPlaced {
    order_id: String,
    customer_id: String,
    items: Vec<LineItem>,
}

impl DomainEvent for OrderPlaced {
    fn event_type(&self) -> &str {
        "OrderPlaced"
    }
}

struct OrderAggregate {
    id: Option<String>,
    version: u64,
    status: String,
    customer_id: String,
    items: Vec<LineItem>,
}

impl BaseAggregate for OrderAggregate {
    type Event = OrderEvent;
    
    fn aggregate_type(&self) -> &str {
        "Order"
    }
    
    fn apply_event(&mut self, event: &Self::Event) {
        match event {
            OrderEvent::Placed(e) => self.on_order_placed(e),
            OrderEvent::Shipped(e) => self.on_order_shipped(e),
            OrderEvent::Cancelled(e) => self.on_order_cancelled(e),
        }
    }
}

impl OrderAggregate {
    // Command
    pub fn place(&mut self, order_id: String, customer_id: String, items: Vec<LineItem>) -> Result<()> {
        if items.is_empty() {
            return Err(Error::InvalidCommand("Order must have at least one item".into()));
        }
        self.initialize(order_id.clone());
        self.raise_event(OrderEvent::Placed(OrderPlaced {
            order_id,
            customer_id,
            items,
        }));
        Ok(())
    }
    
    // Event handler
    fn on_order_placed(&mut self, event: &OrderPlaced) {
        self.status = "placed".to_string();
        self.customer_id = event.customer_id.clone();
        self.items = event.items.clone();
    }
}
```

## 📦 Installation (Future)

```toml
[dependencies]
neurale-event-sourcing = "0.1"
```

## 🎯 Design Goals

- **Performance**: Zero-cost abstractions with minimal runtime overhead
- **Safety**: Leverage Rust's type system for compile-time guarantees
- **Ergonomics**: Idiomatic Rust with macros for reducing boilerplate
- **Async**: First-class async/await support
- **Testing**: Built-in test utilities and mocking

## 🔗 Related

- **[TypeScript SDK](../typescript/typescript-sdk.md)** - Currently available
- **[API Reference](../api-reference.md)** - Common API surface
- **[Event Store Rust SDK](/docs/event-store/sdks/rust/rust-sdk.md)** - Low-level event store client

---

**Interested in contributing?** Check out our [GitHub repository](https://github.com/neurale/event-sourcing-platform) for contribution guidelines.
