import { Event } from '@event-sourcing-platform/core';

export interface MoneyDepositedEventData {
  accountId: string;
  amount: number;
}

@Event('MoneyDeposited', 'v1')
export class MoneyDepositedEvent {
  readonly eventType = 'MoneyDeposited' as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public readonly aggregateId: string,
    public readonly data: MoneyDepositedEventData,
    public readonly timestamp: Date = new Date()
  ) {}
}

