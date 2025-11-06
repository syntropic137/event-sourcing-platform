import { CommandBus } from '../../infrastructure/CommandBus';
import { CreateOrderCommand } from '../../domain/commands/CreateOrderCommand';

/**
 * Thin CLI adapter for creating orders.
 * No business logic - just translates CLI input to domain command.
 */
export class CreateOrderCli {
  constructor(private commandBus: CommandBus) {}

  async execute(orderId: string, customerId: string, items: string[]): Promise<void> {
    const command = new CreateOrderCommand(orderId, customerId, items);
    await this.commandBus.dispatch(command);
    console.log(`Order ${orderId} created successfully`);
  }
}

