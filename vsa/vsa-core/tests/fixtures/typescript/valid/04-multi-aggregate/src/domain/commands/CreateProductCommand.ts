import { Command } from '@event-sourcing-platform/core';

@Command('CreateProduct', 'Create a new product')
export class CreateProductCommand {
  constructor(
    public readonly productId: string,
    public readonly name: string,
    public readonly price: number,
    public readonly stock: number
  ) {}
}

