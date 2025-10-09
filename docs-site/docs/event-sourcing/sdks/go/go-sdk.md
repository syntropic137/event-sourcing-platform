# Go SDK (Coming Soon)

The Go SDK for Event Sourcing is currently in development.

## 🚧 Status

**Planned Features:**
- Idiomatic Go interfaces
- Goroutine-safe operations
- Context-based cancellation
- Struct-based event definitions
- Testing utilities with table-driven tests
- Full compatibility with Event Store gRPC API

## 📋 Planned API

```go
package main

import (
    "context"
    "github.com/neurale/event-sourcing-go"
)

// Event definition
type OrderPlaced struct {
    OrderID    string
    CustomerID string
    Items      []LineItem
}

func (e OrderPlaced) EventType() string {
    return "OrderPlaced"
}

// Aggregate
type OrderAggregate struct {
    eventsourcing.BaseAggregate
    status     string
    customerID string
    items      []LineItem
}

func (a *OrderAggregate) AggregateType() string {
    return "Order"
}

// Command
func (a *OrderAggregate) Place(ctx context.Context, orderID, customerID string, items []LineItem) error {
    if len(items) == 0 {
        return errors.New("order must have at least one item")
    }
    
    a.Initialize(orderID)
    return a.RaiseEvent(ctx, OrderPlaced{
        OrderID:    orderID,
        CustomerID: customerID,
        Items:      items,
    })
}

// Event handler
func (a *OrderAggregate) ApplyEvent(event eventsourcing.DomainEvent) error {
    switch e := event.(type) {
    case OrderPlaced:
        a.status = "placed"
        a.customerID = e.CustomerID
        a.items = e.Items
    case OrderShipped:
        a.status = "shipped"
    case OrderCancelled:
        a.status = "cancelled"
    }
    return nil
}

// Usage
func main() {
    ctx := context.Background()
    
    // Setup
    client := eventsourcing.NewGrpcClient("localhost:50051")
    repo := eventsourcing.NewRepository(client, "Order")
    
    // Create order
    order := &OrderAggregate{}
    err := order.Place(ctx, "order-123", "customer-1", []LineItem{
        {ProductID: "prod-1", Quantity: 2, Price: 29.99},
    })
    if err != nil {
        panic(err)
    }
    
    // Save
    err = repo.Save(ctx, order)
    if err != nil {
        panic(err)
    }
    
    // Load
    loaded, err := repo.Load(ctx, "order-123")
    if err != nil {
        panic(err)
    }
    
    fmt.Printf("Order status: %s\n", loaded.status)
}
```

## 📦 Installation (Future)

```bash
go get github.com/neurale/event-sourcing-go
```

## 🎯 Design Goals

- **Simplicity**: Clear, idiomatic Go code
- **Concurrency**: Safe concurrent access with goroutines
- **Context**: Proper context propagation for cancellation and timeouts
- **Testing**: Easy to test with interfaces and mocking
- **Performance**: Efficient memory usage and minimal allocations

## 🔗 Related

- **[TypeScript SDK](../typescript/typescript-sdk.md)** - Currently available
- **[API Reference](../api-reference.md)** - Common API surface


---

**Interested in contributing?** Check out our [GitHub repository](https://github.com/neurale/event-sourcing-platform) for contribution guidelines.
