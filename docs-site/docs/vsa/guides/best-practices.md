---
sidebar_position: 1
---

# Best Practices

Recommended patterns and practices for VSA development.

## Project Structure

### ✅ Keep Features Focused

```
✅ Good: Small, focused features
place-order/
cancel-order/
update-shipping-address/

❌ Bad: Large, unfocused features
order-management/  (too broad)
```

### ✅ Use Descriptive Names

```
✅ Good: Clear, descriptive
place-order/
cancel-order-with-refund/
update-customer-shipping-address/

❌ Bad: Abbreviated or unclear
plc-ord/
cancel/
update/
```

## Bounded Contexts

### ✅ Clear Boundaries

- One team per context
- Separate databases
- Communicate via events only
- No shared models

### ✅ Right-Sized Contexts

```
✅ Good: Focused contexts
- orders (order processing)
- inventory (stock management)
- shipping (shipment tracking)

❌ Bad: Too large
- warehouse (inventory + shipping + receiving)
```

## Integration Events

### ✅ Minimal Contracts

```typescript
✅ Good: Minimal, stable
interface OrderPlaced {
  orderId: string;
  totalAmount: number;
  timestamp: Date;
}

❌ Bad: Too much detail
interface OrderPlaced {
  orderId: string;
  customerId: string;
  items: OrderItem[];
  shippingAddress: Address;
  paymentMethod: PaymentMethod;
  discount: Discount;
  // ... 20 more fields
}
```

### ✅ Version Events

```typescript
✅ Good: Versioned
class OrderPlaced {
  readonly version = 2;
  // ...
}

❌ Bad: Unversioned
class OrderPlaced {
  // No version tracking
}
```

## Testing

### ✅ Test Pyramid

```
        E2E Tests (few)
       /              \
    Integration Tests
   /                    \
  Unit Tests (many)
```

### ✅ Test First

```typescript
// 1. Write test
it('should place order', async () => {
  await handler.handle(command);
  expect(events).toHaveLength(1);
});

// 2. Implement
export class PlaceOrderHandler {
  async handle(command: PlaceOrderCommand) {
    // Implementation
  }
}
```

## Code Organization

### ✅ Co-locate Related Code

```
place-order/
├── PlaceOrderCommand.ts
├── OrderPlacedEvent.ts
├── PlaceOrderHandler.ts
└── PlaceOrder.test.ts
```

### ✅ Minimize Shared Code

```
contexts/orders/
├── place-order/
├── cancel-order/
└── _shared/              ← Only truly shared
    └── OrderAggregate.ts
```

## Validation

### ✅ Enable in CI/CD

```yaml
# .github/workflows/ci.yml
- name: Validate VSA
  run: vsa validate
```

### ✅ Use Watch Mode

```bash
# During development
vsa validate --watch
```

## Documentation

### ✅ Document Events

```typescript
/**
 * Published when an order is successfully placed.
 * 
 * @publisher orders
 * @subscribers inventory, shipping, notifications
 */
export class OrderPlaced {
  // ...
}
```

### ✅ Keep Architecture Updated

```bash
# Generate manifest regularly
vsa manifest --output docs/architecture.json
```

## Common Anti-Patterns to Avoid

### ❌ God Context

Don't put everything in one context:
```
❌ contexts/app/  (contains everything)
```

### ❌ Anemic Handlers

Don't just pass data through:
```typescript
❌ class PlaceOrderHandler {
  async handle(cmd: PlaceOrderCommand) {
    // No validation, no business logic
    await this.db.save(cmd);
  }
}
```

### ❌ Shared Database

Don't share databases between contexts:
```
❌ contexts/orders/ → shared_db
❌ contexts/inventory/ → shared_db
```

### ❌ Synchronous Cross-Context Calls

Don't call other contexts directly:
```typescript
❌ const product = await catalogService.getProduct(id);
✅ // Use integration events and local read models
```

## Summary

**Do:**
- Keep features focused
- Use clear names
- Define bounded contexts
- Communicate via events
- Test comprehensively
- Validate continuously

**Don't:**
- Create god contexts
- Share databases
- Use synchronous cross-context calls
- Skip tests
- Ignore validation errors

---

**More guides:**
- Troubleshooting - See documentation and examples
- Migrating to VSA - Contact team for migration assistance

