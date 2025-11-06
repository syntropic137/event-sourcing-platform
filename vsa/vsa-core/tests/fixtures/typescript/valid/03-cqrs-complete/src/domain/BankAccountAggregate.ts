import { OpenAccountCommand } from './commands/OpenAccountCommand';
import { DepositMoneyCommand } from './commands/DepositMoneyCommand';
import { AccountOpenedEvent } from './events/AccountOpenedEvent';
import { MoneyDepositedEvent } from './events/MoneyDepositedEvent';

export class BankAccountAggregate {
  private accountId?: string;
  private customerId?: string;
  private balance = 0;
  private isOpen = false;

  // Command handlers
  openAccount(command: OpenAccountCommand): AccountOpenedEvent {
    if (this.isOpen) {
      throw new Error('Account already open');
    }

    return new AccountOpenedEvent(
      command.accountId,
      {
        accountId: command.accountId,
        customerId: command.customerId,
        initialBalance: command.initialBalance,
      }
    );
  }

  depositMoney(command: DepositMoneyCommand): MoneyDepositedEvent {
    if (!this.isOpen) {
      throw new Error('Account not open');
    }

    return new MoneyDepositedEvent(
      command.accountId,
      {
        accountId: command.accountId,
        amount: command.amount,
      }
    );
  }

  // Event appliers
  apply(event: AccountOpenedEvent | MoneyDepositedEvent): void {
    if (event.eventType === 'AccountOpened') {
      this.accountId = event.data.accountId;
      this.customerId = event.data.customerId;
      this.balance = event.data.initialBalance;
      this.isOpen = true;
    } else if (event.eventType === 'MoneyDeposited') {
      this.balance += event.data.amount;
    }
  }

  // Getters
  getBalance(): number {
    return this.balance;
  }

  getCustomerId(): string | undefined {
    return this.customerId;
  }
}

