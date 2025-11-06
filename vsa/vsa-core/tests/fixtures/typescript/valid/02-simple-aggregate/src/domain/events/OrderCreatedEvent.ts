import { Event } from '@event-sourcing-platform/core';

export interface OrderCreatedEventData {
    orderId: string;
    customerId: string;
    items: string[];
}

@Event('OrderCreated', 'v1')
export class OrderCreatedEvent {
    readonly eventType = 'OrderCreated' as const;
    readonly schemaVersion = 1 as const;

    constructor(
        public readonly aggregateId: string,
        public readonly data: OrderCreatedEventData,
        public readonly timestamp: Date = new Date()
    ) { }
}

