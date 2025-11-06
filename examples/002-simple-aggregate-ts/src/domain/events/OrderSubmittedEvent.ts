import { BaseDomainEvent, Event } from "@neuralempowerment/event-sourcing-typescript";

/**
 * Domain Event: OrderSubmitted
 * 
 * Emitted when an order is successfully submitted.
 * 
 * @Event decorator specifies event type and version for:
 * - Event versioning and upcasting (ADR-007)
 * - Framework auto-discovery
 * - Serialization/deserialization
 */
@Event("OrderSubmitted", "v1")
export class OrderSubmittedEvent extends BaseDomainEvent {
  readonly eventType = "OrderSubmitted" as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public readonly orderId: string,
    public readonly customerId: string,
  ) {
    super();
  }
}

