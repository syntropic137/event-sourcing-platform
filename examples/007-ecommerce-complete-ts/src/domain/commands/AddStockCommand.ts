export class AddStockCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly quantity: number
  ) {}
}

