import { BaseDomainEvent, Event } from "@event-sourcing-platform/typescript";

@Event("MoneyDeposited", "v1")
export class MoneyDepositedEvent extends BaseDomainEvent {
  readonly eventType = "MoneyDeposited" as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public amount: number,
    public description: string,
    public transactionId: string,
  ) {
    super();
  }
}

