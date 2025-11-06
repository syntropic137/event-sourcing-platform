import { BaseDomainEvent, Event } from "@event-sourcing-platform/typescript";

/**
 * Domain Event: StockAdded
 * 
 * Emitted when inventory is added to a product.
 */
@Event("StockAdded", "v1")
export class StockAddedEvent extends BaseDomainEvent {
  readonly eventType = "StockAdded" as const;
  readonly schemaVersion = 1 as const;

  constructor(public readonly quantity: number) {
    super();
  }
}

