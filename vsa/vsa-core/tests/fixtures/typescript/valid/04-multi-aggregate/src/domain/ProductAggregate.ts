import { CreateProductCommand } from './commands/CreateProductCommand';
import { ProductCreatedEvent } from './events/ProductCreatedEvent';

export class ProductAggregate {
  private productId?: string;
  private name?: string;
  private price = 0;
  private stock = 0;
  private isCreated = false;

  createProduct(command: CreateProductCommand): ProductCreatedEvent {
    if (this.isCreated) {
      throw new Error('Product already created');
    }

    return new ProductCreatedEvent(
      command.productId,
      {
        productId: command.productId,
        name: command.name,
        price: command.price,
        stock: command.stock,
      }
    );
  }

  apply(event: ProductCreatedEvent): void {
    this.productId = event.data.productId;
    this.name = event.data.name;
    this.price = event.data.price;
    this.stock = event.data.stock;
    this.isCreated = true;
  }

  getProductId(): string | undefined {
    return this.productId;
  }

  getStock(): number {
    return this.stock;
  }
}

