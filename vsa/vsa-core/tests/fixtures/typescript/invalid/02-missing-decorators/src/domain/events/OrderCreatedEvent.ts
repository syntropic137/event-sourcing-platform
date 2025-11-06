// Invalid: Missing @Event decorator

export interface OrderCreatedEventData {
  orderId: string;
  customerId: string;
}

// ❌ Should have @Event('OrderCreated', 'v1') decorator
export class OrderCreatedEvent {
  readonly eventType = 'OrderCreated' as const;
  // ❌ Missing schemaVersion

  constructor(
    public readonly aggregateId: string,
    public readonly data: OrderCreatedEventData,
    public readonly timestamp: Date = new Date()
  ) {}
}

