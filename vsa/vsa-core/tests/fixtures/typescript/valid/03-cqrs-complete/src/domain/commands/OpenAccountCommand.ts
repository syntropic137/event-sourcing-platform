import { Command } from '@event-sourcing-platform/core';

@Command('OpenAccount', 'Open a new bank account')
export class OpenAccountCommand {
  constructor(
    public readonly accountId: string,
    public readonly customerId: string,
    public readonly initialBalance: number
  ) {}
}

