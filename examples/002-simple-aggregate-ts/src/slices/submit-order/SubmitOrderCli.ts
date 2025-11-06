import { CommandBus } from "../../infrastructure/CommandBus";
import { SubmitOrderCommand } from "../../domain/commands/SubmitOrderCommand";

/**
 * Submit Order CLI Adapter
 * 
 * Thin adapter that translates CLI input into SubmitOrderCommand.
 * 
 * ADR-008: Vertical Slices as Hexagonal Adapters
 * - < 50 lines (thin adapter)
 * - No business logic (validation in aggregate)
 * - Dispatches command via CommandBus
 * - Only responsibility: protocol translation (CLI → Command)
 */
export class SubmitOrderCli {
  constructor(private commandBus: CommandBus) {}

  /**
   * Execute submit order command.
   * 
   * @param orderId - Unique order identifier
   * @param customerId - Customer identifier
   */
  async execute(orderId: string, customerId: string): Promise<void> {
    // Create command
    const command = new SubmitOrderCommand(orderId, orderId, customerId);

    // Dispatch to domain via CommandBus
    await this.commandBus.send(command);

    // Log success (adapter responsibility)
    console.log(`✅ Order ${orderId} submitted for customer ${customerId}`);
  }
}

