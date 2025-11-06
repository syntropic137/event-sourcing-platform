import { BaseDomainEvent, Event } from "@neuralempowerment/event-sourcing-typescript";

/**
 * Domain Event: StockRemoved
 * 
 * Emitted when inventory is removed from a product (e.g., for an order).
 */
@Event("StockRemoved", "v1")
export class StockRemovedEvent extends BaseDomainEvent {
  readonly eventType = "StockRemoved" as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public readonly quantity: number,
    public readonly orderId: string
  ) {
    super();
  }
}

