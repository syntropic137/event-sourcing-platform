import { BaseDomainEvent, Event } from "@event-sourcing-platform/typescript";

/**
 * Domain Event: ProductCreated
 * 
 * Emitted when a new product is created in the catalog.
 */
@Event("ProductCreated", "v1")
export class ProductCreatedEvent extends BaseDomainEvent {
  readonly eventType = "ProductCreated" as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public readonly productId: string,
    public readonly name: string,
    public readonly description: string,
    public readonly price: number,
    public readonly stock: number
  ) {
    super();
  }
}

