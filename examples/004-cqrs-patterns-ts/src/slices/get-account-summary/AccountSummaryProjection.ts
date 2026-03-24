import { AccountSummary } from "../../infrastructure/QueryBus";

export class AccountSummaryProjection {
  private accountSummaries = new Map<string, AccountSummary>();

  private onAccountOpened(accountId: string, event: any, timestamp: Date): void {
    this.accountSummaries.set(accountId, {
      accountId,
      customerId: event.customerId,
      accountType: event.accountType,
      balance: event.initialBalance,
      status: "Open",
      transactionCount: 0,
      lastActivity: timestamp,
    });
  }

  private onMoneyDeposited(accountId: string, event: any, timestamp: Date): void {
    const summary = this.accountSummaries.get(accountId);
    if (summary) {
      summary.balance += event.amount;
      summary.transactionCount++;
      summary.lastActivity = timestamp;
    }
  }

  private onMoneyWithdrawn(accountId: string, event: any, timestamp: Date): void {
    const summary = this.accountSummaries.get(accountId);
    if (summary) {
      summary.balance -= event.amount;
      summary.transactionCount++;
      summary.lastActivity = timestamp;
    }
  }

  private onAccountClosed(accountId: string, _event: any, timestamp: Date): void {
    const summary = this.accountSummaries.get(accountId);
    if (summary) {
      summary.status = "Closed";
      summary.lastActivity = timestamp;
    }
  }

  processEvents(events: any[]): void {
    const handlers: Record<string, (accountId: string, event: any, timestamp: Date) => void> = {
      AccountOpened: (id, e, ts) => this.onAccountOpened(id, e, ts),
      MoneyDeposited: (id, e, ts) => this.onMoneyDeposited(id, e, ts),
      MoneyWithdrawn: (id, e, ts) => this.onMoneyWithdrawn(id, e, ts),
      AccountClosed: (id, e, ts) => this.onAccountClosed(id, e, ts),
    };

    for (const envelope of events) {
      const handler = handlers[envelope.event.eventType];
      if (handler) {
        handler(envelope.metadata.aggregateId, envelope.event, new Date(envelope.metadata.timestamp));
      }
    }
  }

  getAccountSummary(accountId: string): AccountSummary | undefined {
    return this.accountSummaries.get(accountId);
  }

  getAllAccountSummaries(): AccountSummary[] {
    return Array.from(this.accountSummaries.values());
  }

  getAccountsByCustomer(customerId: string): AccountSummary[] {
    return Array.from(this.accountSummaries.values()).filter(
      (account) => account.customerId === customerId,
    );
  }
}

