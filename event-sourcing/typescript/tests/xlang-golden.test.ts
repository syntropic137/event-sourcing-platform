/**
 * Golden cross-language fixtures (ADR-027), decoded by the TypeScript SDK.
 *
 * `event-sourcing/rust/tests/fixtures/xlang/*.json` hold the `AppendRequest`
 * written by the real TypeScript, Python and Rust encoders, plus
 * `typescript-legacy.json` (TS SDK 0.17, payloads echo eventType and
 * schemaVersion). The Rust golden tests check the encoders agree byte for
 * byte (and that `append_request` describes the bytes); this checks the TS
 * reader decodes every one. Regenerate with
 * `make -C event-sourcing/rust test-xlang-fixtures`.
 */

import fs from 'fs';
import path from 'path';

import {
  BaseDomainEvent,
  Event,
  EventFactory,
  EventSerializer,
  UnknownEventVersionError,
  Upcasters,
} from '../src';
import { decodeGrpcEvent } from '../src/integrations/grpc-event-store';
import { captureAdapter } from './helpers/capture-adapter';

const FIXTURES = path.resolve(__dirname, '../../rust/tests/fixtures/xlang');
const PRODUCERS = ['typescript', 'python', 'rust', 'typescript-legacy'];
const NOTE = 'café ☕ "quoted"';

// Fixture domain (mirrors event-sourcing/rust/tests/common/xlang.rs).
@Event('AccountOpened', 'v1')
class AccountOpened extends BaseDomainEvent {
  readonly eventType = 'AccountOpened' as const;
  readonly schemaVersion = 1 as const;
  account_id = '';
  owner = '';
}

@Event('MoneyDeposited', 'v1')
class MoneyDeposited extends BaseDomainEvent {
  readonly eventType = 'MoneyDeposited' as const;
  readonly schemaVersion = 1 as const;
  amount = 0;
  note = '';
}

@Event('AccountClosed', 'v2')
class AccountClosed extends BaseDomainEvent {
  readonly eventType = 'AccountClosed' as const;
  readonly schemaVersion = 2 as const;
  reason = '';
  tags: string[] = [];
}

interface FixtureEvent {
  meta: Record<string, string | number>;
  payload: string;
}

interface Fixture {
  producer: string;
  append_request: {
    tenantId: string;
    aggregateId: string;
    aggregateType: string;
    events: FixtureEvent[];
  };
  payloads: string[];
}

function load(producer: string): Fixture {
  const fixture = JSON.parse(
    fs.readFileSync(path.join(FIXTURES, `${producer}.json`), 'utf8')
  ) as Fixture;
  expect(fixture.producer).toBe(producer);
  return fixture;
}

/** The fixture event as the gRPC client hands it to the adapter. */
function wire(e: FixtureEvent) {
  const m = e.meta;
  return {
    meta: {
      eventId: String(m.eventId),
      aggregateId: String(m.aggregateId),
      aggregateType: String(m.aggregateType ?? ''),
      aggregateNonce: Number(m.aggregateNonce),
      eventType: String(m.eventType),
      eventVersion: Number(m.eventVersion ?? 0),
      contentType: String(m.contentType ?? ''),
      tenantId: String(m.tenantId ?? ''),
      timestampUnixMs: Number(m.timestampUnixMs ?? 0),
    },
    payload: Buffer.from(e.payload, 'base64'),
  };
}

function expected(aggregateId: string) {
  return [
    Object.assign(new AccountOpened(), { account_id: aggregateId, owner: 'alice' }),
    Object.assign(new MoneyDeposited(), { amount: 125, note: NOTE }),
    Object.assign(new AccountClosed(), { reason: 'done', tags: ['a', 'b'] }),
  ];
}

describe.each(PRODUCERS)('fixture %s', (producer) => {
  const fixture = load(producer);
  const req = fixture.append_request;

  it('decodes into the registered classes, by type and version', () => {
    const envelopes = req.events.map((e) => decodeGrpcEvent(wire(e), 'Account'));
    const want = expected(req.aggregateId);
    expect(envelopes.map((e) => e.event.constructor)).toEqual([
      AccountOpened,
      MoneyDeposited,
      AccountClosed,
    ]);
    envelopes.forEach((env, i) => {
      expect(env.event).toEqual(want[i]);
      expect(env.event.toJson()).toEqual(want[i].toJson());
      expect(env.metadata.storedEventType).toBe(req.events[i].meta.eventType);
      expect(env.metadata.storedEventVersion).toBe(Number(req.events[i].meta.eventVersion));
      expect(env.event.schemaVersion).toBe(Number(req.events[i].meta.eventVersion));
      expect(env.metadata.aggregateType).toBe('Account');
    });
  });

  it('upcasts the v1 deposit to a v2 class', () => {
    class MoneyDepositedV2 extends BaseDomainEvent {
      readonly eventType = 'MoneyDeposited' as const;
      readonly schemaVersion = 2 as const;
      amount = 0;
      note = '';
      currency = '';
    }
    const deposit = wire(req.events[1]);
    const registry = (EventSerializer as unknown as { eventRegistry: Map<string, unknown[]> })
      .eventRegistry;
    const saved = registry.get('MoneyDeposited');
    try {
      // Only v2 is known: the stored v1 needs the upcaster.
      registry.set('MoneyDeposited', []);
      EventSerializer.registerEvent('MoneyDeposited', MoneyDepositedV2);
      const up = new Upcasters().register('MoneyDeposited', 1, 2, (b) => ({
        ...b,
        currency: 'EUR',
      }));
      const env = decodeGrpcEvent(deposit, 'Account', up);
      expect(env.event).toBeInstanceOf(MoneyDepositedV2);
      expect(env.event.toJson()).toEqual({ amount: 125, note: NOTE, currency: 'EUR' });
      expect(env.metadata.storedEventVersion).toBe(1);
      expect(() => decodeGrpcEvent(deposit, 'Account')).toThrow(UnknownEventVersionError);
    } finally {
      registry.set('MoneyDeposited', saved ?? []);
    }
  });
});

it('typescript.json is what the TypeScript adapter writes today', async () => {
  const fixture = load('typescript');
  const req = fixture.append_request;
  const { adapter, sent } = captureAdapter(req.tenantId);
  const envelopes = expected(req.aggregateId).map((ev, i) => {
    const m = req.events[i].meta;
    return {
      event: ev,
      metadata: {
        ...EventFactory.create(ev, {
          aggregateId: req.aggregateId,
          aggregateType: 'Account',
          aggregateNonce: i + 1,
          tenantId: req.tenantId,
          eventTimestamp: new Date(Number(m.timestampUnixMs)).toISOString(),
        }).metadata,
        eventId: String(m.eventId),
      },
    };
  });
  await adapter.appendEvents(`Account-${req.aggregateId}`, envelopes);
  expect(sent).toHaveLength(1);
  sent[0].events.forEach((e, i) => {
    const m = req.events[i].meta;
    expect(e.meta.eventType).toBe(m.eventType);
    expect(e.meta.eventVersion).toBe(Number(m.eventVersion));
    expect(e.meta.contentType).toBe(m.contentType);
    expect(e.meta.timestampUnixMs).toBe(Number(m.timestampUnixMs));
    // Byte-identical payload: regenerate the fixture if this changes.
    expect(e.payload.toString('utf8')).toBe(fixture.payloads[i]);
  });
});
