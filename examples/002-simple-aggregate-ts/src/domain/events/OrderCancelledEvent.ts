import { BaseDomainEvent, Event } from "@event-sourcing-platform/typescript";

/**
 * Domain Event: OrderCancelled
 * 
 * Emitted when an order is cancelled.
 * 
 * @Event decorator specifies event type and version for:
 * - Event versioning and upcasting (ADR-007)
 * - Framework auto-discovery
 * - Serialization/deserialization
 */
@Event("OrderCancelled", "v1")
export class OrderCancelledEvent extends BaseDomainEvent {
  readonly eventType = "OrderCancelled" as const;
  readonly schemaVersion = 1 as const;

  constructor(public readonly reason: string) {
    super();
  }
}

