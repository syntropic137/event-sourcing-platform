import { BaseDomainEvent, Event } from "@neuralempowerment/event-sourcing-typescript";

@Event("MoneyWithdrawn", "v1")
export class MoneyWithdrawnEvent extends BaseDomainEvent {
  readonly eventType = "MoneyWithdrawn" as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public amount: number,
    public description: string,
    public transactionId: string,
  ) {
    super();
  }
}

