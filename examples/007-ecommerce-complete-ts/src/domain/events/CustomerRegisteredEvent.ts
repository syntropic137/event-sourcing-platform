import { BaseDomainEvent, Event } from "@neuralempowerment/event-sourcing-typescript";

/**
 * Domain Event: CustomerRegistered
 * 
 * Emitted when a new customer is registered.
 */
@Event("CustomerRegistered", "v1")
export class CustomerRegisteredEvent extends BaseDomainEvent {
  readonly eventType = "CustomerRegistered" as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public readonly customerId: string,
    public readonly email: string,
    public readonly name: string,
    public readonly address: string
  ) {
    super();
  }
}

