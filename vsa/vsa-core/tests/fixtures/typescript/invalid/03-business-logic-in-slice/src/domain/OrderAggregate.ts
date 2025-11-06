import { CreateOrderCommand } from './commands/CreateOrderCommand';
import { OrderCreatedEvent } from './events/OrderCreatedEvent';

export class OrderAggregate {
  private orderId?: string;

  createOrder(command: CreateOrderCommand): OrderCreatedEvent {
    return new OrderCreatedEvent(
      command.orderId,
      {
        orderId: command.orderId,
        customerId: command.customerId,
        totalAmount: command.totalAmount,
      }
    );
  }

  apply(event: OrderCreatedEvent): void {
    this.orderId = event.data.orderId;
  }
}

