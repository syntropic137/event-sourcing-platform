import { BaseDomainEvent, Event } from "@event-sourcing-platform/typescript";

/**
 * Domain Event: OrderItemAdded
 * 
 * Emitted when an item is added to an order.
 */
@Event("OrderItemAdded", "v1")
export class OrderItemAddedEvent extends BaseDomainEvent {
  readonly eventType = "OrderItemAdded" as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public readonly productId: string,
    public readonly productName: string,
    public readonly quantity: number,
    public readonly pricePerUnit: number
  ) {
    super();
  }
}

