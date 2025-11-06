import { CommandBus } from "../../infrastructure/CommandBus";
import { DepositMoneyCommand } from "../../domain/commands/DepositMoneyCommand";

export class DepositMoneyCli {
  constructor(private commandBus: CommandBus) {}

  async handle(
    accountId: string,
    amount: number,
    description: string,
  ): Promise<void> {
    const command = new DepositMoneyCommand(accountId, amount, description);
    await this.commandBus.send(command);
    console.log(`💰 Deposited $${amount} to account ${accountId}`);
  }
}

