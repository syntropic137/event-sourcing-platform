import { BaseDomainEvent, Event } from "@neuralempowerment/event-sourcing-typescript";

/**
 * Domain Event: OrderCreated
 * 
 * Emitted when a new order is created.
 */
@Event("OrderCreated", "v1")
export class OrderCreatedEvent extends BaseDomainEvent {
  readonly eventType = "OrderCreated" as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public readonly orderId: string,
    public readonly customerId: string
  ) {
    super();
  }
}

