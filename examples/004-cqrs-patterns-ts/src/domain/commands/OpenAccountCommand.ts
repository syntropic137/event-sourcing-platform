export class OpenAccountCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly customerId: string,
    public readonly accountType: string,
    public readonly initialBalance: number,
  ) {}
}

