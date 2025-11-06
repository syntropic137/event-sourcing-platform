import { BaseDomainEvent, Event } from "@event-sourcing-platform/typescript";

/**
 * Domain Event: OrderShipped
 * 
 * Emitted when an order is shipped with a tracking number.
 */
@Event("OrderShipped", "v1")
export class OrderShippedEvent extends BaseDomainEvent {
  readonly eventType = "OrderShipped" as const;
  readonly schemaVersion = 1 as const;

  constructor(public readonly trackingNumber: string) {
    super();
  }
}

