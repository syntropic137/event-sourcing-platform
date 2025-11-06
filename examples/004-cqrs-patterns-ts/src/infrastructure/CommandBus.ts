import {
  EventStoreClient,
  RepositoryFactory,
} from "@neuralempowerment/event-sourcing-typescript";
import { BankAccountAggregate } from "../domain/BankAccountAggregate";
import { OpenAccountCommand } from "../domain/commands/OpenAccountCommand";
import { DepositMoneyCommand } from "../domain/commands/DepositMoneyCommand";
import { WithdrawMoneyCommand } from "../domain/commands/WithdrawMoneyCommand";
import { CloseAccountCommand } from "../domain/commands/CloseAccountCommand";

type SupportedCommands =
  | OpenAccountCommand
  | DepositMoneyCommand
  | WithdrawMoneyCommand
  | CloseAccountCommand;

export class CommandBus {
  private repositoryFactory: RepositoryFactory;

  constructor(eventStoreClient: EventStoreClient) {
    this.repositoryFactory = new RepositoryFactory(eventStoreClient);
  }

  async send(command: SupportedCommands): Promise<void> {
    const repository = this.repositoryFactory.createRepository(
      () => new BankAccountAggregate(),
      "BankAccount",
    );

    let aggregate = await repository.load(command.aggregateId);

    // For OpenAccountCommand, create new aggregate if not exists
    if (!aggregate) {
      aggregate = new BankAccountAggregate();
    }

    (aggregate as any).handleCommand(command);
    await repository.save(aggregate);
  }
}

