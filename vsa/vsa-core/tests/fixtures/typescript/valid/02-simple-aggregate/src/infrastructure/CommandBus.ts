import { OrderAggregate } from '../domain/OrderAggregate';
import { CreateOrderCommand } from '../domain/commands/CreateOrderCommand';

export class CommandBus {
  private aggregate: OrderAggregate;

  constructor() {
    this.aggregate = new OrderAggregate();
  }

  async dispatch(command: CreateOrderCommand): Promise<void> {
    const event = this.aggregate.createOrder(command);
    this.aggregate.apply(event);
  }

  getAggregate(): OrderAggregate {
    return this.aggregate;
  }
}

