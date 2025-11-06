import { CommandBus } from '../../../infrastructure/CommandBus';
import { OpenAccountCommand } from '../../../domain/commands/OpenAccountCommand';

/**
 * Thin CLI adapter for opening accounts.
 * No business logic - just translates CLI input to domain command.
 */
export class OpenAccountCli {
  constructor(private commandBus: CommandBus) {}

  async execute(accountId: string, customerId: string, initialBalance: number): Promise<void> {
    const command = new OpenAccountCommand(accountId, customerId, initialBalance);
    await this.commandBus.dispatch(command);
    console.log(`Account ${accountId} opened successfully`);
  }
}

