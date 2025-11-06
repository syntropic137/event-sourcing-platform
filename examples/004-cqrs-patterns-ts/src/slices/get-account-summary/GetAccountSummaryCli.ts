import { QueryBus, AccountSummary } from "../../infrastructure/QueryBus";
import { GetAccountSummaryQuery } from "../../domain/queries/GetAccountSummaryQuery";

export class GetAccountSummaryCli {
  constructor(private queryBus: QueryBus) {}

  async handle(accountId: string): Promise<void> {
    const query = new GetAccountSummaryQuery(accountId);
    const summary = await this.queryBus.send<AccountSummary>(query);

    if (!summary) {
      console.log(`❌ Account ${accountId} not found`);
      return;
    }

    console.log(`\n💳 Account Summary:`);
    console.log(`   Account ID: ${summary.accountId}`);
    console.log(`   Customer ID: ${summary.customerId}`);
    console.log(`   Type: ${summary.accountType}`);
    console.log(`   Balance: $${summary.balance}`);
    console.log(`   Status: ${summary.status}`);
    console.log(`   Transactions: ${summary.transactionCount}`);
    if (summary.lastActivity) {
      console.log(`   Last Activity: ${summary.lastActivity.toISOString()}`);
    }
  }
}

