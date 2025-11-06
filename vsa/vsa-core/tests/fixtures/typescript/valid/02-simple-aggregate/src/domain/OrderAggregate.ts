import { CreateOrderCommand } from './commands/CreateOrderCommand';
import { OrderCreatedEvent } from './events/OrderCreatedEvent';

export class OrderAggregate {
  private orderId?: string;
  private customerId?: string;
  private items: string[] = [];
  private isCreated = false;

  // Command handler
  createOrder(command: CreateOrderCommand): OrderCreatedEvent {
    if (this.isCreated) {
      throw new Error('Order already created');
    }

    return new OrderCreatedEvent(
      command.orderId,
      {
        orderId: command.orderId,
        customerId: command.customerId,
        items: command.items,
      }
    );
  }

  // Event applier
  apply(event: OrderCreatedEvent): void {
    this.orderId = event.data.orderId;
    this.customerId = event.data.customerId;
    this.items = event.data.items;
    this.isCreated = true;
  }

  // Getters
  getOrderId(): string | undefined {
    return this.orderId;
  }

  getItems(): string[] {
    return [...this.items];
  }
}

