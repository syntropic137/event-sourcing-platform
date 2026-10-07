/**
 * Upcasters (ADR-007, ADR-027): same semantics as the Rust SDK's Upcasters.
 */

import { UpcastError, Upcasters, type JsonObject } from '../src';

const addCurrency = (body: JsonObject): JsonObject => ({ ...body, currency: 'EUR' });

describe('Upcasters.register', () => {
  it('runs a single step', () => {
    const up = new Upcasters().register('MoneyDeposited', 1, 2, addCurrency);
    expect(up.upcast('MoneyDeposited', 1, { amount: 5 })).toEqual({
      eventType: 'MoneyDeposited',
      eventVersion: 2,
      payload: { amount: 5, currency: 'EUR' },
    });
  });

  it('chains steps', () => {
    const up = new Upcasters()
      .register('E', 1, 2, (b) => ({ ...b, v2: true }))
      .register('E', 2, 3, (b) => ({ ...b, v3: true }));
    expect(up.upcast('E', 1, {})).toEqual({
      eventType: 'E',
      eventVersion: 3,
      payload: { v2: true, v3: true },
    });
    expect(up.upcast('E', 2, {}).payload).toEqual({ v3: true });
  });

  it('reads version 0 as 1', () => {
    const up = new Upcasters().register('E', 1, 2, addCurrency);
    expect(up.handles('E', 0)).toBe(true);
    expect(up.upcast('E', 0, {}).eventVersion).toBe(2);
  });

  it('passes unmatched events through', () => {
    const up = new Upcasters().register('E', 1, 2, addCurrency);
    const body = { x: 1 };
    expect(up.upcast('Other', 1, body)).toEqual({
      eventType: 'Other',
      eventVersion: 1,
      payload: body,
    });
    expect(up.upcast('E', 2, body).eventVersion).toBe(2);
    expect(up.handles('E', 2)).toBe(false);
    expect(new Upcasters().isEmpty()).toBe(true);
  });

  it.each([
    [2, 2],
    [2, 1],
    [0, 1],
  ])('rejects %i -> %i', (from, to) => {
    expect(() => new Upcasters().register('E', from, to, addCurrency)).toThrow();
  });

  it('rejects duplicates', () => {
    const up = new Upcasters().register('E', 1, 2, addCurrency);
    expect(() => up.register('E', 1, 3, addCurrency)).toThrow(/duplicate/);
  });

  it.each(['', 'has space', 'café'])('rejects invalid type name %p', (name) => {
    expect(() => new Upcasters().register(name, 1, 2, addCurrency)).toThrow(/invalid event type/);
  });
});

describe('Upcasters.rename', () => {
  it('changes the type', () => {
    const up = new Upcasters().rename('OldName', 1, 'NewName', 1, (b) => b);
    expect(up.upcast('OldName', 1, { a: 1 })).toEqual({
      eventType: 'NewName',
      eventVersion: 1,
      payload: { a: 1 },
    });
  });

  it('chains with register', () => {
    const up = new Upcasters()
      .rename('OldName', 1, 'NewName', 1, (b) => b)
      .register('NewName', 1, 2, addCurrency);
    expect(up.upcast('OldName', 1, {})).toEqual({
      eventType: 'NewName',
      eventVersion: 2,
      payload: { currency: 'EUR' },
    });
  });

  it('must change the type', () => {
    expect(() => new Upcasters().rename('E', 1, 'E', 2, addCurrency)).toThrow(/use register/);
  });

  it('detects cycles', () => {
    const up = new Upcasters().rename('A', 1, 'B', 1, (b) => b).rename('B', 1, 'A', 1, (b) => b);
    expect(() => up.upcast('A', 1, {})).toThrow(UpcastError);
  });
});

describe('Upcasters failures', () => {
  it('wraps a throwing step', () => {
    const up = new Upcasters().register('E', 1, 2, () => {
      throw new Error('nope');
    });
    let caught: unknown;
    try {
      up.upcast('E', 1, {});
    } catch (err) {
      caught = err;
    }
    expect(caught).toBeInstanceOf(UpcastError);
    const e = caught as UpcastError;
    expect([e.eventType, e.eventVersion, e.code]).toEqual(['E', 1, 'UPCAST_ERROR']);
    expect(e.message).toMatch(/nope/);
  });

  it('rejects a step that returns a non-object', () => {
    const up = new Upcasters().register('E', 1, 2, () => [1] as unknown as JsonObject);
    expect(() => up.upcast('E', 1, {})).toThrow(/JSON object/);
  });

  it('rejects a non-object payload', () => {
    const up = new Upcasters().register('E', 1, 2, addCurrency);
    expect(() => up.upcast('E', 1, [1] as unknown as JsonObject)).toThrow(/not a JSON object/);
  });
});
