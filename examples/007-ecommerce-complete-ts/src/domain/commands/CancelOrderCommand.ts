export class CancelOrderCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly reason: string
  ) {}
}

