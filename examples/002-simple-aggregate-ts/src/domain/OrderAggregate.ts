import {
  Aggregate,
  AggregateRoot,
  CommandHandler,
  EventSourcingHandler,
} from "@event-sourcing-platform/typescript";
import { SubmitOrderCommand } from "./commands/SubmitOrderCommand";
import { CancelOrderCommand } from "./commands/CancelOrderCommand";
import { OrderSubmittedEvent } from "./events/OrderSubmittedEvent";
import { OrderCancelledEvent } from "./events/OrderCancelledEvent";

/**
 * Order Domain Status
 */
export enum OrderStatus {
  New = "New",
  Submitted = "Submitted",
  Cancelled = "Cancelled",
}

/**
 * Order Aggregate
 * 
 * Represents the order lifecycle in the domain.
 * 
 * Key patterns (ADR-004, ADR-006):
 * - Lives in domain/ folder (shared across all slices)
 * - Command handlers (@CommandHandler) contain business logic and validation
 * - Event handlers (@EventSourcingHandler) update state only
 * - Never accessed directly by slices (use CommandBus instead)
 * 
 * Business Rules:
 * - Orders must be submitted before they can be cancelled
 * - Once cancelled, orders cannot be resubmitted
 */
@Aggregate("Order")
export class OrderAggregate extends AggregateRoot<
  OrderSubmittedEvent | OrderCancelledEvent
> {
  private status: OrderStatus = OrderStatus.New;

  /**
   * Command Handler: Submit Order
   * 
   * Business logic:
   * - Initializes new order
   * - Validates order is in 'New' state
   * - Emits OrderSubmittedEvent
   */
  @CommandHandler("SubmitOrderCommand")
  submit(command: SubmitOrderCommand): void {
    // Initialize aggregate if new
    if (!this.aggregateId) {
      this.initialize(command.orderId);
    }

    // Business rule: Can only submit new orders
    if (this.status !== OrderStatus.New) {
      throw new Error(
        `Cannot submit order: Order is in '${this.status}' state, expected '${OrderStatus.New}'`,
      );
    }

    // Emit event (triggers @EventSourcingHandler)
    this.apply(new OrderSubmittedEvent(command.orderId, command.customerId));
  }

  /**
   * Command Handler: Cancel Order
   * 
   * Business logic:
   * - Validates order is in 'Submitted' state
   * - Emits OrderCancelledEvent
   */
  @CommandHandler("CancelOrderCommand")
  cancel(command: CancelOrderCommand): void {
    // Business rule: Can only cancel submitted orders
    if (this.status !== OrderStatus.Submitted) {
      throw new Error(
        `Cannot cancel order: Order is in '${this.status}' state, expected '${OrderStatus.Submitted}'`,
      );
    }

    // Emit event (triggers @EventSourcingHandler)
    this.apply(new OrderCancelledEvent(command.reason));
  }

  /**
   * Event Sourcing Handler: Order Submitted
   * 
   * State update only - no validation or business logic.
   */
  @EventSourcingHandler("OrderSubmitted")
  private onSubmitted(event: OrderSubmittedEvent): void {
    if (!this.aggregateId) {
      this.initialize(event.orderId);
    }
    this.status = OrderStatus.Submitted;
  }

  /**
   * Event Sourcing Handler: Order Cancelled
   * 
   * State update only - no validation or business logic.
   */
  @EventSourcingHandler("OrderCancelled")
  private onCancelled(): void {
    this.status = OrderStatus.Cancelled;
  }

  /**
   * Query method for reading aggregate state.
   * Used by projections and queries.
   */
  getStatus(): OrderStatus {
    return this.status;
  }
}

