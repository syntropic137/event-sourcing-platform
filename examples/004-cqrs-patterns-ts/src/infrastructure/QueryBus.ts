import { GetAccountSummaryQuery } from "../domain/queries/GetAccountSummaryQuery";
import { GetTransactionHistoryQuery } from "../domain/queries/GetTransactionHistoryQuery";
import { GetAccountsByCustomerQuery } from "../domain/queries/GetAccountsByCustomerQuery";

// Type definitions for read models
export interface AccountSummary {
  accountId: string;
  customerId: string;
  accountType: string;
  balance: number;
  status: string;
  transactionCount: number;
  lastActivity?: Date;
}

export interface TransactionHistory {
  transactionId: string;
  accountId: string;
  type: "deposit" | "withdrawal";
  amount: number;
  description: string;
  timestamp: Date;
  balanceAfter: number;
}

type SupportedQueries =
  | GetAccountSummaryQuery
  | GetTransactionHistoryQuery
  | GetAccountsByCustomerQuery;

// Query handler registry
interface QueryHandler<TQuery, TResult> {
  handle(query: TQuery): Promise<TResult | undefined>;
}

export class QueryBus {
  private handlers = new Map<
    string,
    QueryHandler<any, any>
  >();

  registerHandler<TQuery, TResult>(
    queryType: string,
    handler: QueryHandler<TQuery, TResult>,
  ): void {
    this.handlers.set(queryType, handler);
  }

  async send<TResult>(query: SupportedQueries): Promise<TResult | undefined> {
    const queryType = query.constructor.name;
    const handler = this.handlers.get(queryType);

    if (!handler) {
      throw new Error(`No handler registered for query: ${queryType}`);
    }

    return handler.handle(query);
  }
}

