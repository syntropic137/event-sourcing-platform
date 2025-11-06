import { CommandBus } from "../../infrastructure/CommandBus";
import { WithdrawMoneyCommand } from "../../domain/commands/WithdrawMoneyCommand";

export class WithdrawMoneyCli {
  constructor(private commandBus: CommandBus) {}

  async handle(
    accountId: string,
    amount: number,
    description: string,
  ): Promise<void> {
    const command = new WithdrawMoneyCommand(accountId, amount, description);
    await this.commandBus.send(command);
    console.log(`💸 Withdrew $${amount} from account ${accountId}`);
  }
}

