import { AccountSummary } from "../../infrastructure/QueryBus";

export class AccountSummaryProjection {
  private accountSummaries = new Map<string, AccountSummary>();

  processEvents(events: any[]): void {
    for (const envelope of events) {
      const event = envelope.event;
      const metadata = envelope.metadata;
      const accountId = metadata.aggregateId;

      switch (event.eventType) {
        case "AccountOpened":
          this.accountSummaries.set(accountId, {
            accountId,
            customerId: event.customerId,
            accountType: event.accountType,
            balance: event.initialBalance,
            status: "Open",
            transactionCount: 0,
            lastActivity: new Date(metadata.timestamp),
          });
          break;

        case "MoneyDeposited": {
          const summary = this.accountSummaries.get(accountId);
          if (summary) {
            summary.balance += event.amount;
            summary.transactionCount++;
            summary.lastActivity = new Date(metadata.timestamp);
          }
          break;
        }

        case "MoneyWithdrawn": {
          const summary = this.accountSummaries.get(accountId);
          if (summary) {
            summary.balance -= event.amount;
            summary.transactionCount++;
            summary.lastActivity = new Date(metadata.timestamp);
          }
          break;
        }

        case "AccountClosed": {
          const summary = this.accountSummaries.get(accountId);
          if (summary) {
            summary.status = "Closed";
            summary.lastActivity = new Date(metadata.timestamp);
          }
          break;
        }
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

