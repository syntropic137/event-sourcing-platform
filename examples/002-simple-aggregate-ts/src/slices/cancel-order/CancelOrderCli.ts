import { CommandBus } from "../../infrastructure/CommandBus";
import { CancelOrderCommand } from "../../domain/commands/CancelOrderCommand";

/**
 * Cancel Order CLI Adapter
 * 
 * Thin adapter that translates CLI input into CancelOrderCommand.
 * 
 * ADR-008: Vertical Slices as Hexagonal Adapters
 * - < 50 lines (thin adapter)
 * - No business logic (validation in aggregate)
 * - Dispatches command via CommandBus
 * - Only responsibility: protocol translation (CLI → Command)
 */
export class CancelOrderCli {
  constructor(private commandBus: CommandBus) {}

  /**
   * Execute cancel order command.
   * 
   * @param orderId - Order identifier to cancel
   * @param reason - Reason for cancellation
   */
  async execute(orderId: string, reason: string): Promise<void> {
    // Create command
    const command = new CancelOrderCommand(orderId, reason);

    // Dispatch to domain via CommandBus
    await this.commandBus.send(command);

    // Log success (adapter responsibility)
    console.log(`✅ Order ${orderId} cancelled: ${reason}`);
  }
}

