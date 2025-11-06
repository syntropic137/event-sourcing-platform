/**
 * Command: Submit Order
 * 
 * Represents the intent to submit an order.
 * Commands are dispatched via CommandBus to the OrderAggregate.
 * 
 * Key requirements (ADR-004):
 * - Must have `aggregateId` property for routing
 * - Contains all data needed for the operation
 * - Validation happens in aggregate's @CommandHandler method
 */
export class SubmitOrderCommand {
  constructor(
    public readonly aggregateId: string, // Required for CommandBus routing
    public readonly orderId: string,
    public readonly customerId: string,
  ) {}
}

