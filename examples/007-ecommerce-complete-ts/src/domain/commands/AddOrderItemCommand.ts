export class AddOrderItemCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly productId: string,
    public readonly productName: string,
    public readonly quantity: number,
    public readonly pricePerUnit: number
  ) {}
}

