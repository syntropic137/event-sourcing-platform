import { TransactionHistory, AccountSummary } from "../../infrastructure/QueryBus";

export class TransactionHistoryProjection {
  private transactionHistories = new Map<string, TransactionHistory[]>();
  private accountBalances = new Map<string, number>();

  processEvents(events: any[]): void {
    for (const envelope of events) {
      const event = envelope.event;
      const metadata = envelope.metadata;
      const accountId = metadata.aggregateId;

      switch (event.eventType) {
        case "AccountOpened":
          this.accountBalances.set(accountId, event.initialBalance);
          break;

        case "MoneyDeposited": {
          const currentBalance = this.accountBalances.get(accountId) || 0;
          const newBalance = currentBalance + event.amount;
          this.accountBalances.set(accountId, newBalance);

          const transactions = this.transactionHistories.get(accountId) || [];
          transactions.push({
            transactionId: event.transactionId,
            accountId,
            type: "deposit",
            amount: event.amount,
            description: event.description,
            timestamp: new Date(metadata.timestamp),
            balanceAfter: newBalance,
          });
          this.transactionHistories.set(accountId, transactions);
          break;
        }

        case "MoneyWithdrawn": {
          const currentBalance = this.accountBalances.get(accountId) || 0;
          const newBalance = currentBalance - event.amount;
          this.accountBalances.set(accountId, newBalance);

          const transactions = this.transactionHistories.get(accountId) || [];
          transactions.push({
            transactionId: event.transactionId,
            accountId,
            type: "withdrawal",
            amount: event.amount,
            description: event.description,
            timestamp: new Date(metadata.timestamp),
            balanceAfter: newBalance,
          });
          this.transactionHistories.set(accountId, transactions);
          break;
        }
      }
    }
  }

  getTransactionHistory(accountId: string): TransactionHistory[] {
    return this.transactionHistories.get(accountId) || [];
  }
}

