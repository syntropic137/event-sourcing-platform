import {
  Aggregate,
  AggregateRoot,
  CommandHandler,
  EventSourcingHandler,
} from "@event-sourcing-platform/typescript";
import { CustomerRegisteredEvent } from "./events/CustomerRegisteredEvent";
import { CustomerAddressUpdatedEvent } from "./events/CustomerAddressUpdatedEvent";
import { RegisterCustomerCommand } from "./commands/RegisterCustomerCommand";
import { UpdateCustomerAddressCommand } from "./commands/UpdateCustomerAddressCommand";

type CustomerEvent = CustomerRegisteredEvent | CustomerAddressUpdatedEvent;

/**
 * CustomerAggregate
 * 
 * Manages customer registration and profile information.
 */
@Aggregate("Customer")
export class CustomerAggregate extends AggregateRoot<CustomerEvent> {
  private email = "";
  private name = "";
  private address = "";

  getAggregateType(): string {
    return "Customer";
  }

  @CommandHandler("RegisterCustomerCommand")
  registerCustomer(command: RegisterCustomerCommand): void {
    if (!command.email || !command.email.includes("@"))
      throw new Error("Valid email is required");
    if (!command.name) throw new Error("Name is required");
    if (!command.address) throw new Error("Address is required");
    if (this.id !== null) throw new Error("Customer already registered");

    this.initialize(command.aggregateId);
    this.apply(
      new CustomerRegisteredEvent(
        command.aggregateId,
        command.email,
        command.name,
        command.address
      )
    );
  }

  @CommandHandler("UpdateCustomerAddressCommand")
  updateAddress(command: UpdateCustomerAddressCommand): void {
    if (this.id === null) throw new Error("Customer does not exist");
    if (!command.newAddress) throw new Error("New address is required");
    if (command.newAddress === this.address)
      throw new Error("New address is same as current address");

    this.apply(
      new CustomerAddressUpdatedEvent(this.address, command.newAddress)
    );
  }

  @EventSourcingHandler("CustomerRegistered")
  private onCustomerRegistered(event: CustomerRegisteredEvent): void {
    this.email = event.email;
    this.name = event.name;
    this.address = event.address;
  }

  @EventSourcingHandler("CustomerAddressUpdated")
  private onAddressUpdated(event: CustomerAddressUpdatedEvent): void {
    this.address = event.newAddress;
  }

  getEmail(): string {
    return this.email;
  }
  getName(): string {
    return this.name;
  }
  getAddress(): string {
    return this.address;
  }
}

