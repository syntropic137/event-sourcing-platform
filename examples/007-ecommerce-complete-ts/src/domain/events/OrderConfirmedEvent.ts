import { BaseDomainEvent, Event } from "@neuralempowerment/event-sourcing-typescript";

/**
 * Domain Event: OrderConfirmed
 * 
 * Emitted when an order is confirmed and ready for fulfillment.
 */
@Event("OrderConfirmed", "v1")
export class OrderConfirmedEvent extends BaseDomainEvent {
  readonly eventType = "OrderConfirmed" as const;
  readonly schemaVersion = 1 as const;

  constructor(public readonly totalAmount: number) {
    super();
  }
}

