import { BaseDomainEvent, Event } from "@event-sourcing-platform/typescript";

@Event("AccountOpened", "v1")
export class AccountOpenedEvent extends BaseDomainEvent {
  readonly eventType = "AccountOpened" as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public accountId: string,
    public customerId: string,
    public accountType: string,
    public initialBalance: number,
  ) {
    super();
  }
}

