import { BaseDomainEvent, Event } from "@neuralempowerment/event-sourcing-typescript";

@Event("AccountClosed", "v1")
export class AccountClosedEvent extends BaseDomainEvent {
  readonly eventType = "AccountClosed" as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public reason: string,
    public finalBalance: number,
  ) {
    super();
  }
}

