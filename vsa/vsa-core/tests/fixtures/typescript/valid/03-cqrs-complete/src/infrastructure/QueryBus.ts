import { GetAccountBalanceQuery, AccountBalanceResult } from '../domain/queries/GetAccountBalanceQuery';
import { AccountBalanceProjection } from '../slices/queries/account-balance/AccountBalanceProjection';

export class QueryBus {
  constructor(private projection: AccountBalanceProjection) {}

  async execute(query: GetAccountBalanceQuery): Promise<AccountBalanceResult | undefined> {
    return this.projection.getBalance(query.accountId);
  }
}

