export class CreateProductCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly name: string,
    public readonly description: string,
    public readonly price: number,
    public readonly stock: number
  ) {}
}

