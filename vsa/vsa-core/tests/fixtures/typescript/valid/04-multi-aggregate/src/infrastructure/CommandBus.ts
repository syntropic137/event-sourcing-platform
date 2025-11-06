import { ProductAggregate } from '../domain/ProductAggregate';
import { OrderAggregate } from '../domain/OrderAggregate';
import { CreateProductCommand } from '../domain/commands/CreateProductCommand';
import { CreateOrderCommand } from '../domain/commands/CreateOrderCommand';

export class CommandBus {
  private products = new Map<string, ProductAggregate>();
  private orders = new Map<string, OrderAggregate>();

  async dispatch(command: CreateProductCommand | CreateOrderCommand): Promise<void> {
    if (command instanceof CreateProductCommand) {
      let aggregate = this.products.get(command.productId);
      if (!aggregate) {
        aggregate = new ProductAggregate();
        this.products.set(command.productId, aggregate);
      }
      const event = aggregate.createProduct(command);
      aggregate.apply(event);
    } else if (command instanceof CreateOrderCommand) {
      let aggregate = this.orders.get(command.orderId);
      if (!aggregate) {
        aggregate = new OrderAggregate();
        this.orders.set(command.orderId, aggregate);
      }
      const event = aggregate.createOrder(command);
      aggregate.apply(event);
    }
  }

  getProduct(productId: string): ProductAggregate | undefined {
    return this.products.get(productId);
  }

  getOrder(orderId: string): OrderAggregate | undefined {
    return this.orders.get(orderId);
  }
}

