import { Event } from '@event-sourcing-platform/core';

export interface ProductCreatedEventData {
  productId: string;
  name: string;
  price: number;
  stock: number;
}

@Event('ProductCreated', 'v1')
export class ProductCreatedEvent {
  readonly eventType = 'ProductCreated' as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public readonly aggregateId: string,
    public readonly data: ProductCreatedEventData,
    public readonly timestamp: Date = new Date()
  ) {}
}

