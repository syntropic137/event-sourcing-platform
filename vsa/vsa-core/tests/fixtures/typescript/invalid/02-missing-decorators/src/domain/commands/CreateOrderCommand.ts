// Invalid: Missing @Command decorator

// ❌ Should have @Command('CreateOrder', 'Create a new order') decorator
export class CreateOrderCommand {
  constructor(
    public readonly orderId: string,
    public readonly customerId: string
  ) {}
}

