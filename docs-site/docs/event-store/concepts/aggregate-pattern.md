# Aggregate & @EventSourcingHandler Pattern

This repo uses a classic event-sourced aggregate model:

- **Commands**: intent to change state (validate invariants, decide events).
- **Events**: facts produced by commands.
- **Apply/@EventSourcingHandler**: pure functions that evolve state from events.

Key properties:
- Command handlers do not mutate state directly; they return events.
- Apply functions are deterministic, side-effect-free, and order-dependent.
- Rehydration: load stream events and fold them through apply to get current state.

Example (pseudo-TypeScript):

```ts
class OrderState { status: 'New'|'Submitted'|'Cancelled' = 'New'; }

class OrderAggregate {
  private state = new OrderState();
  private version = 0;

  // Command handler
  submit(cmd: { orderId: string }) {
    if (this.state.status !== 'New') throw new Error('Already submitted');
    return [{ type: 'OrderSubmitted', data: { orderId: cmd.orderId } }];
  }

  // @EventSourcingHandler (apply)
  apply(event: { type: string; data: any }) {
    switch (event.type) {
      case 'OrderSubmitted':
        this.state.status = 'Submitted';
        break;
    }
    this.version += 1;
  }
}

// Rehydrate
function fold(events: any[]): OrderAggregate {
  const agg = new OrderAggregate();
  for (const ev of events) agg.apply(ev);
  return agg;
}
```

Notes:
- In strongly typed languages, prefer one `applyX` per event with a decorator/registry to route events.
- Persist events with optimistic concurrency (send the current stream head as `expectedAggregateNonce`).
- On write: rehydrate -> handle command -> append events -> new version.
