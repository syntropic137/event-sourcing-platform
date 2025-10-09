import { EventFactory, EventSerializer, BaseDomainEvent } from '../src';

describe('EventSerializer edge cases', () => {
  class SimpleEvent extends BaseDomainEvent {
    readonly eventType = 'SimpleEvent' as const;
    readonly schemaVersion = 1 as const;
    payload: Record<string, unknown> = {};
  }

  beforeAll(() => {
    EventSerializer.registerEvent('SimpleEvent', SimpleEvent as unknown as new () => SimpleEvent);
  });

  it('round-trips event envelopes with minimal metadata', () => {
    const event = new SimpleEvent();
    event.payload = { foo: 'bar' };

    const envelope = EventFactory.create(event, {
      aggregateId: 'agg-1',
      aggregateType: 'TestAggregate',
      aggregateNonce: 1,
    });

    const serialised = EventSerializer.serialize(envelope);
    const deserialised = EventSerializer.deserialize(serialised);

    expect(deserialised.event.eventType).toBe('SimpleEvent');
    expect((deserialised.event as SimpleEvent).payload).toEqual({ foo: 'bar' });
    expect(deserialised.metadata.aggregateId).toBe('agg-1');
    expect(deserialised.metadata.aggregateNonce).toBe(1);
  });

  it('throws when deserialising an unregistered event type', () => {
    expect(() =>
      EventSerializer.deserialize({
        event: {
          eventType: 'UnknownEvent',
          schemaVersion: 1,
          data: {},
        },
        metadata: {
          eventId: 'evt-1',
          timestamp: new Date().toISOString(),
          recordedTimestamp: new Date().toISOString(),
          aggregateVersion: 1,
          aggregateId: 'agg-unknown',
          aggregateType: 'Unknown',
          contentType: 'application/json',
          headers: {},
          customMetadata: {},
        },
      })
    ).toThrow('Unknown event type: UnknownEvent');
  });
});
