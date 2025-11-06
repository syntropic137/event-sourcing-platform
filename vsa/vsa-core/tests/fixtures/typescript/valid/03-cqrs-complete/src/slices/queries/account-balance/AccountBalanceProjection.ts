import { AccountOpenedEvent } from '../../../domain/events/AccountOpenedEvent';
import { MoneyDepositedEvent } from '../../../domain/events/MoneyDepositedEvent';
import { AccountBalanceResult } from '../../../domain/queries/GetAccountBalanceQuery';

/**
 * Read model projection for account balances.
 * Listens to events and maintains denormalized view.
 */
export class AccountBalanceProjection {
  private balances = new Map<string, AccountBalanceResult>();

  handleAccountOpened(event: AccountOpenedEvent): void {
    this.balances.set(event.data.accountId, {
      accountId: event.data.accountId,
      balance: event.data.initialBalance,
      customerId: event.data.customerId,
    });
  }

  handleMoneyDeposited(event: MoneyDepositedEvent): void {
    const existing = this.balances.get(event.data.accountId);
    if (existing) {
      this.balances.set(event.data.accountId, {
        ...existing,
        balance: existing.balance + event.data.amount,
      });
    }
  }

  getBalance(accountId: string): AccountBalanceResult | undefined {
    return this.balances.get(accountId);
  }
}

