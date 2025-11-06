import { Event } from '@event-sourcing-platform/core';

export interface AccountOpenedEventData {
  accountId: string;
  customerId: string;
  initialBalance: number;
}

@Event('AccountOpened', 'v1')
export class AccountOpenedEvent {
  readonly eventType = 'AccountOpened' as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public readonly aggregateId: string,
    public readonly data: AccountOpenedEventData,
    public readonly timestamp: Date = new Date()
  ) {}
}

