import { CommandBus } from "../../infrastructure/CommandBus";
import { CloseAccountCommand } from "../../domain/commands/CloseAccountCommand";

export class CloseAccountCli {
  constructor(private commandBus: CommandBus) {}

  async handle(accountId: string, reason: string): Promise<void> {
    const command = new CloseAccountCommand(accountId, reason);
    await this.commandBus.send(command);
    console.log(`🔒 Closed account ${accountId}`);
  }
}

