import { CommandBus } from "../../infrastructure/CommandBus";
import { OpenAccountCommand } from "../../domain/commands/OpenAccountCommand";

export class OpenAccountCli {
  constructor(private commandBus: CommandBus) {}

  async handle(
    accountId: string,
    customerId: string,
    accountType: string,
    initialBalance: number,
  ): Promise<void> {
    const command = new OpenAccountCommand(
      accountId,
      customerId,
      accountType,
      initialBalance,
    );
    await this.commandBus.send(command);
    console.log(
      `✅ Opened ${accountType} account ${accountId} with $${initialBalance}`,
    );
  }
}

