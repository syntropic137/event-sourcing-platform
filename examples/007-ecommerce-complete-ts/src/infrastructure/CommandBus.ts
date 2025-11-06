import {
  EventStoreClient,
  RepositoryFactory,
} from "@neuralempowerment/event-sourcing-typescript";
import { ProductAggregate } from "../domain/ProductAggregate";
import { OrderAggregate } from "../domain/OrderAggregate";
import { CustomerAggregate } from "../domain/CustomerAggregate";
import { CreateProductCommand } from "../domain/commands/CreateProductCommand";
import { UpdateProductPriceCommand } from "../domain/commands/UpdateProductPriceCommand";
import { AddStockCommand } from "../domain/commands/AddStockCommand";
import { RemoveStockCommand } from "../domain/commands/RemoveStockCommand";
import { CreateOrderCommand } from "../domain/commands/CreateOrderCommand";
import { AddOrderItemCommand } from "../domain/commands/AddOrderItemCommand";
import { ConfirmOrderCommand } from "../domain/commands/ConfirmOrderCommand";
import { ShipOrderCommand } from "../domain/commands/ShipOrderCommand";
import { CancelOrderCommand } from "../domain/commands/CancelOrderCommand";
import { RegisterCustomerCommand } from "../domain/commands/RegisterCustomerCommand";
import { UpdateCustomerAddressCommand } from "../domain/commands/UpdateCustomerAddressCommand";

type SupportedCommands =
  | CreateProductCommand
  | UpdateProductPriceCommand
  | AddStockCommand
  | RemoveStockCommand
  | CreateOrderCommand
  | AddOrderItemCommand
  | ConfirmOrderCommand
  | ShipOrderCommand
  | CancelOrderCommand
  | RegisterCustomerCommand
  | UpdateCustomerAddressCommand;

/**
 * CommandBus
 * 
 * Routes commands to the appropriate aggregate based on command type.
 * Part of the infrastructure layer (hexagonal architecture).
 */
export class CommandBus {
  private repositoryFactory: RepositoryFactory;

  constructor(eventStoreClient: EventStoreClient) {
    this.repositoryFactory = new RepositoryFactory(eventStoreClient);
  }

  async send(command: SupportedCommands): Promise<void> {
    const commandName = command.constructor.name;

    // Product commands
    if (
      commandName === "CreateProductCommand" ||
      commandName === "UpdateProductPriceCommand" ||
      commandName === "AddStockCommand" ||
      commandName === "RemoveStockCommand"
    ) {
      const repository = this.repositoryFactory.createRepository(
        () => new ProductAggregate(),
        "Product"
      );
      let aggregate = await repository.load(command.aggregateId);
      if (!aggregate) {
        aggregate = new ProductAggregate();
      }
      (aggregate as any).handleCommand(command);
      await repository.save(aggregate);
      return;
    }

    // Order commands
    if (
      commandName === "CreateOrderCommand" ||
      commandName === "AddOrderItemCommand" ||
      commandName === "ConfirmOrderCommand" ||
      commandName === "ShipOrderCommand" ||
      commandName === "CancelOrderCommand"
    ) {
      const repository = this.repositoryFactory.createRepository(
        () => new OrderAggregate(),
        "Order"
      );
      let aggregate = await repository.load(command.aggregateId);
      if (!aggregate) {
        aggregate = new OrderAggregate();
      }
      (aggregate as any).handleCommand(command);
      await repository.save(aggregate);
      return;
    }

    // Customer commands
    if (
      commandName === "RegisterCustomerCommand" ||
      commandName === "UpdateCustomerAddressCommand"
    ) {
      const repository = this.repositoryFactory.createRepository(
        () => new CustomerAggregate(),
        "Customer"
      );
      let aggregate = await repository.load(command.aggregateId);
      if (!aggregate) {
        aggregate = new CustomerAggregate();
      }
      (aggregate as any).handleCommand(command);
      await repository.save(aggregate);
      return;
    }

    throw new Error(`No handler registered for command: ${commandName}`);
  }
}

