import { QueryBus, TransactionHistory } from "../../infrastructure/QueryBus";
import { GetTransactionHistoryQuery } from "../../domain/queries/GetTransactionHistoryQuery";

export class GetTransactionHistoryCli {
  constructor(private queryBus: QueryBus) {}

  async handle(accountId: string): Promise<void> {
    const query = new GetTransactionHistoryQuery(accountId);
    const transactions = await this.queryBus.send<TransactionHistory[]>(query);

    if (!transactions || transactions.length === 0) {
      console.log(`📋 No transactions found for account ${accountId}`);
      return;
    }

    console.log(`\n📋 Transaction History for ${accountId}:`);
    transactions.forEach((tx: any, i: number) => {
      const sign = tx.type === "deposit" ? "+" : "-";
      console.log(
        `   ${i + 1}. ${sign}$${tx.amount} - ${tx.description} (Balance: $${tx.balanceAfter})`,
      );
    });
  }
}

