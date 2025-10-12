import { AutoDispatchAggregate, BaseDomainEvent, EventSourcingHandler } from '../../src';

export class OrderSubmitted extends BaseDomainEvent {
  readonly eventType = 'OrderSubmitted' as const;
  readonly schemaVersion = 1 as const;
  constructor(
    public readonly orderId: string,
    public readonly customerId: string
  ) {
    super();
  }
}

export class OrderCancelled extends BaseDomainEvent {
  readonly eventType = 'OrderCancelled' as const;
  readonly schemaVersion = 1 as const;
  constructor(public readonly reason: string) {
    super();
  }
}

export type OrderEvent = OrderSubmitted | OrderCancelled;

export enum OrderStatus {
  New = 'New',
  Submitted = 'Submitted',
  Cancelled = 'Cancelled',
}

export class OrderAggregate extends AutoDispatchAggregate<OrderEvent> {
  private status: OrderStatus = OrderStatus.New;

  getAggregateType(): string {
    return 'Order';
  }

  submit(orderId: string, customerId: string): void {
    if (!this.id) {
      this.initialize(orderId);
    }
    if (this.status !== OrderStatus.New) {
      throw new Error('Invalid state');
    }
    this.raiseEvent(new OrderSubmitted(orderId, customerId));
  }

  cancel(reason: string): void {
    if (this.status !== OrderStatus.Submitted) {
      throw new Error('Invalid state');
    }
    this.raiseEvent(new OrderCancelled(reason));
  }

  @EventSourcingHandler('OrderSubmitted')
  private onSubmitted(event: OrderSubmitted): void {
    if (!this.id) {
      this.initialize(event.orderId);
    }
    this.status = OrderStatus.Submitted;
  }

  @EventSourcingHandler('OrderCancelled')
  private onCancelled(): void {
    this.status = OrderStatus.Cancelled;
  }

  getStatus(): OrderStatus {
    return this.status;
  }
}
