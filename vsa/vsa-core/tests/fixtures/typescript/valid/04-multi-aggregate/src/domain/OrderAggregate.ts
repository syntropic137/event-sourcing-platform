import { CreateOrderCommand } from './commands/CreateOrderCommand';
import { OrderCreatedEvent } from './events/OrderCreatedEvent';

export class OrderAggregate {
  private orderId?: string;
  private customerId?: string;
  private productIds: string[] = [];
  private totalAmount = 0;
  private isCreated = false;

  createOrder(command: CreateOrderCommand): OrderCreatedEvent {
    if (this.isCreated) {
      throw new Error('Order already created');
    }

    return new OrderCreatedEvent(
      command.orderId,
      {
        orderId: command.orderId,
        customerId: command.customerId,
        productIds: command.productIds,
        totalAmount: command.totalAmount,
      }
    );
  }

  apply(event: OrderCreatedEvent): void {
    this.orderId = event.data.orderId;
    this.customerId = event.data.customerId;
    this.productIds = event.data.productIds;
    this.totalAmount = event.data.totalAmount;
    this.isCreated = true;
  }

  getOrderId(): string | undefined {
    return this.orderId;
  }

  getTotalAmount(): number {
    return this.totalAmount;
  }
}

