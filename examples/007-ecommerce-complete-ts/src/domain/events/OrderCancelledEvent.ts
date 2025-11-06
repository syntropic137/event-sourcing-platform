import { BaseDomainEvent, Event } from "@neuralempowerment/event-sourcing-typescript";

/**
 * Domain Event: OrderCancelled
 * 
 * Emitted when an order is cancelled.
 */
@Event("OrderCancelled", "v1")
export class OrderCancelledEvent extends BaseDomainEvent {
  readonly eventType = "OrderCancelled" as const;
  readonly schemaVersion = 1 as const;

  constructor(public readonly reason: string) {
    super();
  }
}

