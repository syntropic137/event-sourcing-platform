// Invalid: Aggregate in wrong location (not in domain/ folder)

export class OrderAggregate {
  private orderId?: string;

  createOrder(orderId: string): void {
    this.orderId = orderId;
  }

  getOrderId(): string | undefined {
    return this.orderId;
  }
}

