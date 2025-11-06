export class CreateOrderCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly customerId: string
  ) {}
}

