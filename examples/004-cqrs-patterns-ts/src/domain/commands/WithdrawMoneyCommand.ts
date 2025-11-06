export class WithdrawMoneyCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly amount: number,
    public readonly description: string,
  ) {}
}

