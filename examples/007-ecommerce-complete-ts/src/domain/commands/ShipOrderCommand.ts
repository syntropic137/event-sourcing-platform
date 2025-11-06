export class ShipOrderCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly trackingNumber: string
  ) {}
}

