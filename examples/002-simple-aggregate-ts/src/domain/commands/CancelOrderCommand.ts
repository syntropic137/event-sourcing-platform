/**
 * Command: Cancel Order
 * 
 * Represents the intent to cancel an order.
 * Commands are dispatched via CommandBus to the OrderAggregate.
 * 
 * Key requirements (ADR-004):
 * - Must have `aggregateId` property for routing
 * - Contains all data needed for the operation
 * - Validation happens in aggregate's @CommandHandler method
 */
export class CancelOrderCommand {
  constructor(
    public readonly aggregateId: string, // Required for CommandBus routing
    public readonly reason: string,
  ) {}
}

