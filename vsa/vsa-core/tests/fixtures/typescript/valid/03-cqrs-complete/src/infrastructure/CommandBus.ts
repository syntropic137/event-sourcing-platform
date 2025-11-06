import { BankAccountAggregate } from '../domain/BankAccountAggregate';
import { OpenAccountCommand } from '../domain/commands/OpenAccountCommand';
import { DepositMoneyCommand } from '../domain/commands/DepositMoneyCommand';
import { AccountOpenedEvent } from '../domain/events/AccountOpenedEvent';
import { MoneyDepositedEvent } from '../domain/events/MoneyDepositedEvent';

export class CommandBus {
  private aggregates = new Map<string, BankAccountAggregate>();
  private events: Array<AccountOpenedEvent | MoneyDepositedEvent> = [];

  async dispatch(command: OpenAccountCommand | DepositMoneyCommand): Promise<void> {
    if (command instanceof OpenAccountCommand) {
      let aggregate = this.aggregates.get(command.accountId);
      if (!aggregate) {
        aggregate = new BankAccountAggregate();
        this.aggregates.set(command.accountId, aggregate);
      }
      const event = aggregate.openAccount(command);
      aggregate.apply(event);
      this.events.push(event);
    } else if (command instanceof DepositMoneyCommand) {
      const aggregate = this.aggregates.get(command.accountId);
      if (!aggregate) {
        throw new Error('Account not found');
      }
      const event = aggregate.depositMoney(command);
      aggregate.apply(event);
      this.events.push(event);
    }
  }

  getAggregate(accountId: string): BankAccountAggregate | undefined {
    return this.aggregates.get(accountId);
  }

  getEvents(): Array<AccountOpenedEvent | MoneyDepositedEvent> {
    return [...this.events];
  }
}

