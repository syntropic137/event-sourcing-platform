export class RemoveStockCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly quantity: number,
    public readonly orderId: string
  ) {}
}

