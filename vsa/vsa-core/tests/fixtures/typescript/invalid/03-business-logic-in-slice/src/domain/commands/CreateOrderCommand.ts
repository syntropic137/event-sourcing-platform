import { Command } from '@event-sourcing-platform/core';

@Command('CreateOrder', 'Create a new order')
export class CreateOrderCommand {
  constructor(
    public readonly orderId: string,
    public readonly customerId: string,
    public readonly totalAmount: number
  ) {}
}

