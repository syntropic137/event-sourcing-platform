/**
 * Cross-language envelope (ADR-027): write the real version and a clean
 * payload; read by (eventType, eventVersion) with upcasting; typed errors.
 */

import {
  BaseDomainEvent,
  Event,
  EventDecodeError,
  EventFactory,
  EventPayloadError,
  EventSerializer,
  UnknownEventTypeError,
  UnknownEventVersionError,
  UnsupportedContentTypeError,
  UpcastError,
  Upcasters,
  decodeEvent,
  encodeEventPayload,
  eventVersionOf,
  type DomainEvent,
  type JsonObject,
} from '../src';
import { decodeGrpcEvent } from '../src/integrations/grpc-event-store';
import { captureAdapter } from './helpers/capture-adapter';

@Event('EnvDeposited', 'v1')
class EnvDepositedV1 extends BaseDomainEvent {
  readonly eventType = 'EnvDeposited' as const;
  readonly schemaVersion = 1 as const;
  amount = 0;
}

@Event('EnvDeposited', 'v2')
class EnvDepositedV2 extends BaseDomainEvent {
  readonly eventType = 'EnvDeposited' as const;
  readonly schemaVersion = 2 as const;
  amount = 0;
  currency = '';
}

@Event('EnvClosed', 'v3')
class EnvClosedV3 extends BaseDomainEvent {
  readonly eventType = 'EnvClosed' as const;
  readonly schemaVersion = 3 as const;
  reason = '';
}

// Constructor needs arguments: the version comes from the decorator.
@Event('EnvNeedsArgs', 'v4')
class EnvNeedsArgs extends BaseDomainEvent {
  readonly eventType = 'EnvNeedsArgs' as const;
  readonly schemaVersion = 4 as const;
  readonly n: number;
  constructor(input: { n: number }) {
    super();
    this.n = input.n;
  }
}

function wire(
  eventType: string,
  payload: unknown,
  opts: { version?: number; contentType?: string; raw?: Uint8Array } = {}
) {
  return {
    meta: {
      eventId: '00000000-0000-4000-8000-000000000001',
      aggregateId: 'a-1',
      aggregateType: 'Account',
      aggregateNonce: 1,
      eventType,
      eventVersion: opts.version ?? 1,
      contentType: opts.contentType ?? 'application/json',
      tenantId: 't',
      timestampUnixMs: 1767323045678,
      globalNonce: 42,
    },
    payload: opts.raw ?? Buffer.from(JSON.stringify(payload)),
  };
}

function expectDecodeError<T>(fn: () => unknown, cls: new (...args: never[]) => T): T {
  try {
    fn();
  } catch (err) {
    expect(err).toBeInstanceOf(cls);
    return err as T;
  }
  throw new Error(`expected ${cls.name}`);
}

describe('registry by (eventType, version)', () => {
  it('keeps every version', () => {
    expect(EventSerializer.registeredVersions('EnvDeposited')).toEqual([1, 2]);
    expect(EventSerializer.resolveEventClass('EnvDeposited', 1)).toBe(EnvDepositedV1);
    expect(EventSerializer.resolveEventClass('EnvDeposited', 2)).toBe(EnvDepositedV2);
  });

  it('falls back to the decorator version when the constructor needs arguments', () => {
    expect(EventSerializer.registeredVersions('EnvNeedsArgs')).toEqual([4]);
    expect(EventSerializer.resolveEventClass('EnvNeedsArgs', 4)).toBe(EnvNeedsArgs);
    // Decoding needs a no-argument constructor: a typed error, not a TypeError.
    expectDecodeError(
      () => decodeGrpcEvent(wire('EnvNeedsArgs', { n: 1 }, { version: 4 }), ''),
      EventDecodeError
    );
  });
});

describe('write', () => {
  it('payload holds only event fields', () => {
    const ev = Object.assign(new EnvDepositedV2(), { amount: 5, currency: 'EUR' });
    expect(ev.toJson()).toEqual({ amount: 5, currency: 'EUR' });
    expect(encodeEventPayload(ev)).toEqual({ amount: 5, currency: 'EUR' });
  });

  it('strips eventType/schemaVersion even from a custom toJson', () => {
    const ev: DomainEvent = {
      eventType: 'Custom',
      schemaVersion: 2,
      toJson: () => ({ eventType: 'Custom', schemaVersion: 2, x: 1 }),
    };
    expect(encodeEventPayload(ev)).toEqual({ x: 1 });
  });

  it('writes schemaVersion, and a missing or 0 version as 1', () => {
    expect(eventVersionOf(new EnvClosedV3())).toBe(3);
    const bare = { eventType: 'X', toJson: () => ({}) } as unknown as DomainEvent;
    expect(eventVersionOf(bare)).toBe(1);
    expect(eventVersionOf({ ...bare, schemaVersion: 0 })).toBe(1);
    expect(() => eventVersionOf({ ...bare, schemaVersion: -1 })).toThrow(/schemaVersion/);
  });

  it('the gRPC adapter sends the clean envelope', async () => {
    const { adapter, sent } = captureAdapter('t');
    const ev = Object.assign(new EnvClosedV3(), { reason: 'done' });
    await adapter.appendEvents('Account-a-1', [
      EventFactory.create(ev, { aggregateId: 'a-1', aggregateType: 'Account', aggregateNonce: 1 }),
    ]);
    const [request] = sent;
    const event = request.events[0];
    expect(event.meta.eventType).toBe('EnvClosed');
    expect(event.meta.eventVersion).toBe(3);
    expect(JSON.parse(event.payload.toString('utf8'))).toEqual({ reason: 'done' });
  });
});

describe('read by version', () => {
  it('decodes each version as its own class', () => {
    const v1 = decodeGrpcEvent(wire('EnvDeposited', { amount: 5 }, { version: 1 }), '');
    const v2 = decodeGrpcEvent(
      wire('EnvDeposited', { amount: 5, currency: 'USD' }, { version: 2 }),
      ''
    );
    expect(v1.event).toBeInstanceOf(EnvDepositedV1);
    expect(v2.event).toBeInstanceOf(EnvDepositedV2);
    expect(v2.event.schemaVersion).toBe(2);
    expect(v2.metadata.storedEventType).toBe('EnvDeposited');
    expect(v2.metadata.storedEventVersion).toBe(2);
  });

  it('reads version 0 as 1', () => {
    const env = decodeGrpcEvent(wire('EnvDeposited', { amount: 5 }, { version: 0 }), '');
    expect(env.event).toBeInstanceOf(EnvDepositedV1);
    expect(env.metadata.storedEventVersion).toBe(1);
  });

  it('an unknown version is a typed error, not the wrong class', () => {
    const err = expectDecodeError(
      () => decodeGrpcEvent(wire('EnvClosed', { reason: 'x' }, { version: 1 }), ''),
      UnknownEventVersionError
    );
    expect([err.eventType, err.eventVersion, err.globalNonce, err.code]).toEqual([
      'EnvClosed',
      1,
      42,
      'UNKNOWN_EVENT_VERSION',
    ]);
  });

  it('a newer version than registered is a typed error', () => {
    expectDecodeError(
      () => decodeGrpcEvent(wire('EnvDeposited', { amount: 5 }, { version: 9 }), ''),
      UnknownEventVersionError
    );
  });
});

describe('upcasting', () => {
  const up = new Upcasters().register('EnvClosed', 1, 3, (b) => ({ ...b, reason: 'legacy' }));

  it('runs before decoding', () => {
    const env = decodeGrpcEvent(wire('EnvClosed', {}, { version: 1 }), '', up);
    expect(env.event).toBeInstanceOf(EnvClosedV3);
    expect((env.event as EnvClosedV3).reason).toBe('legacy');
    expect(env.event.schemaVersion).toBe(3);
    expect(env.metadata.storedEventVersion).toBe(1);
  });

  it('dispatches renames on the new type', () => {
    const rename = new Upcasters().rename('EnvRefunded', 1, 'EnvDeposited', 1, (b) => b);
    const env = decodeGrpcEvent(wire('EnvRefunded', { amount: 3 }), '', rename);
    expect(env.event).toBeInstanceOf(EnvDepositedV1);
    expect(env.event.eventType).toBe('EnvDeposited');
    expect(env.metadata.storedEventType).toBe('EnvRefunded');
  });

  it('upcasters see the payload without envelope echo keys', () => {
    const seen: JsonObject[] = [];
    const spy = new Upcasters().register('EnvClosed', 1, 3, (b) => {
      seen.push({ ...b });
      return { ...b, reason: 'r' };
    });
    decodeGrpcEvent(
      wire('EnvClosed', { eventType: 'EnvClosed', schemaVersion: 1, note: 'n' }),
      '',
      spy
    );
    expect(seen).toEqual([{ note: 'n' }]);
  });

  it('a failing step is a positioned UpcastError', () => {
    const boom = new Upcasters().register('EnvClosed', 1, 3, () => {
      throw new Error('nope');
    });
    const err = expectDecodeError(
      () => decodeGrpcEvent(wire('EnvClosed', {}), '', boom),
      UpcastError
    );
    expect(err.globalNonce).toBe(42);
  });
});

describe('backward compatibility', () => {
  it('reads payloads stored by TS <= 0.17 (eventType/schemaVersion echoed)', () => {
    const legacy = { eventType: 'EnvDeposited', schemaVersion: 2, amount: 5, currency: 'EUR' };
    const env = decodeGrpcEvent(wire('EnvDeposited', legacy, { version: 2 }), '');
    expect(env.event).toBeInstanceOf(EnvDepositedV2);
    expect(env.event.toJson()).toEqual({ amount: 5, currency: 'EUR' });
  });

  it('legacy echo keys never override the decoded class constants', () => {
    // A stored v1 payload claiming schemaVersion 2 is still a v1 event.
    const env = decodeGrpcEvent(
      wire('EnvDeposited', { schemaVersion: 2, amount: 5 }, { version: 1 }),
      ''
    );
    expect(env.event).toBeInstanceOf(EnvDepositedV1);
    expect(env.event.schemaVersion).toBe(1);
  });
});

describe('typed errors, never skipped', () => {
  it.each([
    ['array', Buffer.from('[1]')],
    ['null', Buffer.from('null')],
    ['scalar', Buffer.from('3')],
    ['invalid JSON', Buffer.from('{nope')],
    ['invalid UTF-8', Buffer.from([0xff])],
  ])('payload: %s', (_name, raw) => {
    expectDecodeError(
      () => decodeGrpcEvent(wire('EnvDeposited', null, { raw }), ''),
      EventPayloadError
    );
  });

  it('a non-object payload of an unregistered type is an error too', () => {
    expectDecodeError(
      () => decodeGrpcEvent(wire('EnvUnregistered', null, { raw: Buffer.from('[]') }), ''),
      EventPayloadError
    );
  });

  it('unsupported content type', () => {
    expectDecodeError(
      () =>
        decodeGrpcEvent(
          wire('EnvDeposited', { amount: 1 }, { contentType: 'application/protobuf' }),
          ''
        ),
      UnsupportedContentTypeError
    );
  });

  it.each(['', 'application/json; charset=utf-8', 'Application/JSON'])(
    'JSON content type %p',
    (contentType) => {
      const env = decodeGrpcEvent(wire('EnvDeposited', { amount: 1 }, { contentType }), '');
      expect(env.event).toBeInstanceOf(EnvDepositedV1);
    }
  );

  it('missing metadata is an error, not a dropped event', () => {
    expect(() => decodeGrpcEvent({ payload: Buffer.from('{}') }, '')).toThrow(/no metadata/);
  });

  it('an unregistered type is a generic event carrying type, version and data', () => {
    const env = decodeGrpcEvent(
      wire(
        'EnvUnregistered',
        { eventType: 'EnvUnregistered', schemaVersion: 4, x: 1 },
        { version: 4 }
      ),
      ''
    );
    expect(env.event.eventType).toBe('EnvUnregistered');
    expect(env.event.schemaVersion).toBe(4);
    expect(env.event.toJson()).toEqual({ x: 1 });
    expect(env.metadata.storedEventVersion).toBe(4);
  });

  it('requireRegistered makes an unregistered type an error', () => {
    expectDecodeError(
      () => decodeEvent({ eventType: 'EnvUnregistered', payload: {} }, { requireRegistered: true }),
      UnknownEventTypeError
    );
  });
});
