import {
  Aggregate,
  AggregateRoot,
  CommandHandler,
  EventSourcingHandler,
} from "@event-sourcing-platform/typescript";
import { ProductCreatedEvent } from "./events/ProductCreatedEvent";
import { ProductPriceUpdatedEvent } from "./events/ProductPriceUpdatedEvent";
import { StockAddedEvent } from "./events/StockAddedEvent";
import { StockRemovedEvent } from "./events/StockRemovedEvent";
import { CreateProductCommand } from "./commands/CreateProductCommand";
import { UpdateProductPriceCommand } from "./commands/UpdateProductPriceCommand";
import { AddStockCommand } from "./commands/AddStockCommand";
import { RemoveStockCommand } from "./commands/RemoveStockCommand";

type ProductEvent =
  | ProductCreatedEvent
  | ProductPriceUpdatedEvent
  | StockAddedEvent
  | StockRemovedEvent;

/**
 * ProductAggregate
 * 
 * Manages the lifecycle of products in the e-commerce catalog.
 * Handles product creation, pricing updates, and stock management.
 */
@Aggregate("Product")
export class ProductAggregate extends AggregateRoot<ProductEvent> {
  private name = "";
  private description = "";
  private price = 0;
  private stock = 0;

  getAggregateType(): string {
    return "Product";
  }

  @CommandHandler("CreateProductCommand")
  createProduct(command: CreateProductCommand): void {
    if (!command.name) throw new Error("Product name is required");
    if (command.price < 0) throw new Error("Price cannot be negative");
    if (command.stock < 0) throw new Error("Stock cannot be negative");
    if (this.id !== null) throw new Error("Product already exists");

    this.initialize(command.aggregateId);
    this.apply(
      new ProductCreatedEvent(
        command.aggregateId,
        command.name,
        command.description,
        command.price,
        command.stock
      )
    );
  }

  @CommandHandler("UpdateProductPriceCommand")
  updatePrice(command: UpdateProductPriceCommand): void {
    if (this.id === null) throw new Error("Product does not exist");
    if (command.newPrice < 0) throw new Error("Price cannot be negative");
    if (command.newPrice === this.price)
      throw new Error("New price is same as current price");

    this.apply(new ProductPriceUpdatedEvent(this.price, command.newPrice));
  }

  @CommandHandler("AddStockCommand")
  addStock(command: AddStockCommand): void {
    if (this.id === null) throw new Error("Product does not exist");
    if (command.quantity <= 0) throw new Error("Quantity must be positive");

    this.apply(new StockAddedEvent(command.quantity));
  }

  @CommandHandler("RemoveStockCommand")
  removeStock(command: RemoveStockCommand): void {
    if (this.id === null) throw new Error("Product does not exist");
    if (command.quantity <= 0) throw new Error("Quantity must be positive");
    if (this.stock < command.quantity) {
      throw new Error(
        `Insufficient stock: requested ${command.quantity}, available ${this.stock}`
      );
    }

    this.apply(new StockRemovedEvent(command.quantity, command.orderId));
  }

  @EventSourcingHandler("ProductCreated")
  private onProductCreated(event: ProductCreatedEvent): void {
    this.name = event.name;
    this.description = event.description;
    this.price = event.price;
    this.stock = event.stock;
  }

  @EventSourcingHandler("ProductPriceUpdated")
  private onPriceUpdated(event: ProductPriceUpdatedEvent): void {
    this.price = event.newPrice;
  }

  @EventSourcingHandler("StockAdded")
  private onStockAdded(event: StockAddedEvent): void {
    this.stock += event.quantity;
  }

  @EventSourcingHandler("StockRemoved")
  private onStockRemoved(event: StockRemovedEvent): void {
    this.stock -= event.quantity;
  }

  getStock(): number {
    return this.stock;
  }
  getPrice(): number {
    return this.price;
  }
  getName(): string {
    return this.name;
  }
}

