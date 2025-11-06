export class DepositMoneyCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly amount: number,
    public readonly description: string,
  ) {}
}

