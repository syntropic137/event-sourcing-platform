import { Command } from '@event-sourcing-platform/core';

@Command('DepositMoney', 'Deposit money into an account')
export class DepositMoneyCommand {
  constructor(
    public readonly accountId: string,
    public readonly amount: number
  ) {}
}

