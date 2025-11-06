import {
  Aggregate,
  AggregateRoot,
  CommandHandler,
  EventSourcingHandler,
} from "@neuralempowerment/event-sourcing-typescript";
import { OrderCreatedEvent } from "./events/OrderCreatedEvent";
import { OrderItemAddedEvent } from "./events/OrderItemAddedEvent";
import { OrderConfirmedEvent } from "./events/OrderConfirmedEvent";
import { OrderShippedEvent } from "./events/OrderShippedEvent";
import { OrderCancelledEvent } from "./events/OrderCancelledEvent";
import { CreateOrderCommand } from "./commands/CreateOrderCommand";
import { AddOrderItemCommand } from "./commands/AddOrderItemCommand";
import { ConfirmOrderCommand } from "./commands/ConfirmOrderCommand";
import { ShipOrderCommand } from "./commands/ShipOrderCommand";
import { CancelOrderCommand } from "./commands/CancelOrderCommand";

interface OrderItem {
  productId: string;
  productName: string;
  quantity: number;
  pricePerUnit: number;
}

type OrderEvent =
  | OrderCreatedEvent
  | OrderItemAddedEvent
  | OrderConfirmedEvent
  | OrderShippedEvent
  | OrderCancelledEvent;

enum OrderStatus {
  DRAFT = "DRAFT",
  CONFIRMED = "CONFIRMED",
  SHIPPED = "SHIPPED",
  CANCELLED = "CANCELLED",
}

/**
 * OrderAggregate
 * 
 * Manages the complete lifecycle of customer orders from creation to fulfillment.
 * Implements a state machine: DRAFT → CONFIRMED → SHIPPED (or CANCELLED at any stage).
 */
@Aggregate("Order")
export class OrderAggregate extends AggregateRoot<OrderEvent> {
  private customerId = "";
  private items: OrderItem[] = [];
  private status = OrderStatus.DRAFT;
  private totalAmount = 0;
  private trackingNumber = "";

  getAggregateType(): string {
    return "Order";
  }

  @CommandHandler("CreateOrderCommand")
  createOrder(command: CreateOrderCommand): void {
    if (!command.customerId) throw new Error("Customer ID is required");
    if (this.id !== null) throw new Error("Order already exists");

    this.initialize(command.aggregateId);
    this.apply(new OrderCreatedEvent(command.aggregateId, command.customerId));
  }

  @CommandHandler("AddOrderItemCommand")
  addItem(command: AddOrderItemCommand): void {
    if (this.id === null) throw new Error("Order does not exist");
    if (this.status !== OrderStatus.DRAFT) {
      throw new Error("Cannot add items to a confirmed/shipped/cancelled order");
    }
    if (command.quantity <= 0) throw new Error("Quantity must be positive");
    if (command.pricePerUnit < 0) throw new Error("Price cannot be negative");

    this.apply(
      new OrderItemAddedEvent(
        command.productId,
        command.productName,
        command.quantity,
        command.pricePerUnit
      )
    );
  }

  @CommandHandler("ConfirmOrderCommand")
  confirmOrder(command: ConfirmOrderCommand): void {
    if (this.id === null) throw new Error("Order does not exist");
    if (this.status !== OrderStatus.DRAFT)
      throw new Error("Order is not in DRAFT status");
    if (this.items.length === 0) throw new Error("Cannot confirm empty order");

    const total = this.items.reduce(
      (sum, item) => sum + item.quantity * item.pricePerUnit,
      0
    );
    this.apply(new OrderConfirmedEvent(total));
  }

  @CommandHandler("ShipOrderCommand")
  shipOrder(command: ShipOrderCommand): void {
    if (this.id === null) throw new Error("Order does not exist");
    if (this.status !== OrderStatus.CONFIRMED)
      throw new Error("Order must be confirmed before shipping");
    if (!command.trackingNumber) throw new Error("Tracking number is required");

    this.apply(new OrderShippedEvent(command.trackingNumber));
  }

  @CommandHandler("CancelOrderCommand")
  cancelOrder(command: CancelOrderCommand): void {
    if (this.id === null) throw new Error("Order does not exist");
    if (this.status === OrderStatus.SHIPPED)
      throw new Error("Cannot cancel shipped order");
    if (this.status === OrderStatus.CANCELLED)
      throw new Error("Order is already cancelled");
    if (!command.reason) throw new Error("Cancellation reason is required");

    this.apply(new OrderCancelledEvent(command.reason));
  }

  @EventSourcingHandler("OrderCreated")
  private onOrderCreated(event: OrderCreatedEvent): void {
    this.customerId = event.customerId;
  }

  @EventSourcingHandler("OrderItemAdded")
  private onItemAdded(event: OrderItemAddedEvent): void {
    this.items.push({
      productId: event.productId,
      productName: event.productName,
      quantity: event.quantity,
      pricePerUnit: event.pricePerUnit,
    });
  }

  @EventSourcingHandler("OrderConfirmed")
  private onOrderConfirmed(event: OrderConfirmedEvent): void {
    this.status = OrderStatus.CONFIRMED;
    this.totalAmount = event.totalAmount;
  }

  @EventSourcingHandler("OrderShipped")
  private onOrderShipped(event: OrderShippedEvent): void {
    this.status = OrderStatus.SHIPPED;
    this.trackingNumber = event.trackingNumber;
  }

  @EventSourcingHandler("OrderCancelled")
  private onOrderCancelled(event: OrderCancelledEvent): void {
    this.status = OrderStatus.CANCELLED;
  }

  getStatus(): OrderStatus {
    return this.status;
  }
  getTotalAmount(): number {
    return this.totalAmount;
  }
  getItems(): OrderItem[] {
    return [...this.items];
  }
}

