export class CloseAccountCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly reason: string,
  ) {}
}

