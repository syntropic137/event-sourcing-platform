import { BaseDomainEvent, Event } from "@neuralempowerment/event-sourcing-typescript";

/**
 * Domain Event: ProductPriceUpdated
 * 
 * Emitted when a product's price is updated.
 */
@Event("ProductPriceUpdated", "v1")
export class ProductPriceUpdatedEvent extends BaseDomainEvent {
  readonly eventType = "ProductPriceUpdated" as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public readonly oldPrice: number,
    public readonly newPrice: number
  ) {
    super();
  }
}

