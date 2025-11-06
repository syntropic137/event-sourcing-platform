import { BaseDomainEvent, Event } from "@neuralempowerment/event-sourcing-typescript";

/**
 * Domain Event: CustomerAddressUpdated
 * 
 * Emitted when a customer's address is updated.
 */
@Event("CustomerAddressUpdated", "v1")
export class CustomerAddressUpdatedEvent extends BaseDomainEvent {
  readonly eventType = "CustomerAddressUpdated" as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public readonly oldAddress: string,
    public readonly newAddress: string
  ) {
    super();
  }
}

