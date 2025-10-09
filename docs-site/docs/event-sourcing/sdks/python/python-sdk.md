# Python SDK (Coming Soon)

The Python SDK for Event Sourcing is currently in development.

## 🚧 Status

**Planned Features:**
- Pythonic aggregate base classes
- Decorator-based event handlers
- AsyncIO support
- Type hints and mypy compatibility
- Pytest integration
- Full compatibility with Event Store gRPC API

## 📋 Planned API

```python
from neurale.event_sourcing import BaseAggregate, event_handler
from dataclasses import dataclass

@dataclass
class OrderPlaced:
    order_id: str
    customer_id: str
    items: list

class OrderAggregate(BaseAggregate):
    def __init__(self):
        super().__init__()
        self.status = 'new'
        self.customer_id = ''
        self.items = []
    
    def get_aggregate_type(self) -> str:
        return 'Order'
    
    # Command
    def place(self, order_id: str, customer_id: str, items: list):
        if not items:
            raise ValueError('Order must have at least one item')
        self.initialize(order_id)
        self.raise_event(OrderPlaced(order_id, customer_id, items))
    
    # Event handler
    @event_handler(OrderPlaced)
    def on_order_placed(self, event: OrderPlaced):
        self.status = 'placed'
        self.customer_id = event.customer_id
        self.items = event.items
```

## 📦 Installation (Future)

```bash
pip install neurale-event-sourcing
```

## 🔗 Related

- **[TypeScript SDK](../typescript/typescript-sdk.md)** - Currently available
- **[API Reference](../api-reference.md)** - Common API surface
- **[Event Store Python SDK](/docs/event-store/sdks/python/python-sdk.md)** - Low-level event store client

---

**Interested in contributing?** Check out our [GitHub repository](https://github.com/neurale/event-sourcing-platform) for contribution guidelines.
