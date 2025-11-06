export class UpdateProductPriceCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly newPrice: number
  ) {}
}

