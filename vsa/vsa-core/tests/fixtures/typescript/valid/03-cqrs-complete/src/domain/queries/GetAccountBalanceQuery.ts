import { Query } from '@event-sourcing-platform/core';

@Query('GetAccountBalance', 'Get current account balance')
export class GetAccountBalanceQuery {
  constructor(public readonly accountId: string) {}
}

export interface AccountBalanceResult {
  accountId: string;
  balance: number;
  customerId: string;
}

