import { QueryBus, AccountSummary } from "../../infrastructure/QueryBus";
import { GetAccountsByCustomerQuery } from "../../domain/queries/GetAccountsByCustomerQuery";

export class GetAccountsByCustomerCli {
  constructor(private queryBus: QueryBus) {}

  async handle(customerId: string): Promise<void> {
    const query = new GetAccountsByCustomerQuery(customerId);
    const accounts = await this.queryBus.send<AccountSummary[]>(query);

    if (!accounts || accounts.length === 0) {
      console.log(`👤 No accounts found for customer ${customerId}`);
      return;
    }

    console.log(`\n👤 Customer ${customerId} has ${accounts.length} accounts:`);
    accounts.forEach((account: any) => {
      console.log(
        `   ${account.accountType}: $${account.balance} (${account.status})`,
      );
    });
  }
}

