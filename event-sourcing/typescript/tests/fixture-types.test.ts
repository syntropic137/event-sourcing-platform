/**
 * Tests for fixture validation (extracted helpers)
 * Verifies validateFixture and FixtureValidationError
 */

import { validateFixture, FixtureValidationError } from '../src/testing/fixtures/fixture-types';

const validFixture = {
  description: 'Test order placement',
  aggregateType: 'Order',
  events: [{ type: 'OrderPlaced', version: 'v1', data: { orderId: '123' } }],
};

describe('validateFixture', () => {
  it('should return TestFixture for a valid fixture', () => {
    const result = validateFixture(validFixture, 'test.json');
    expect(result).toBeDefined();
    expect(result.description).toBe('Test order placement');
    expect(result.aggregateType).toBe('Order');
    expect(result.events).toHaveLength(1);
  });

  it('should accept fixture with optional fields', () => {
    const fixture = {
      ...validFixture,
      aggregateId: 'order-1',
      expectedState: { status: 'placed' },
      expectedVersion: 1,
      tags: ['smoke'],
    };
    const result = validateFixture(fixture, 'test.json');
    expect(result.aggregateId).toBe('order-1');
    expect(result.tags).toEqual(['smoke']);
  });

  it('should throw FixtureValidationError for non-object input', () => {
    expect(() => validateFixture(null, 'null.json')).toThrow(FixtureValidationError);
    expect(() => validateFixture('string', 'str.json')).toThrow(FixtureValidationError);
    expect(() => validateFixture(42, 'num.json')).toThrow(FixtureValidationError);
    expect(() => validateFixture(undefined, 'undef.json')).toThrow(FixtureValidationError);
  });

  it('should report missing description field', () => {
    const fixture = { aggregateType: 'Order', events: [] };
    try {
      validateFixture(fixture, 'test.json');
      throw new Error('Expected FixtureValidationError');
    } catch (e) {
      expect(e).toBeInstanceOf(FixtureValidationError);
      expect((e as FixtureValidationError).issues).toContain('description must be a string');
    }
  });

  it('should report missing aggregateType field', () => {
    const fixture = { description: 'Test', events: [] };
    try {
      validateFixture(fixture, 'test.json');
      throw new Error('Expected FixtureValidationError');
    } catch (e) {
      expect(e).toBeInstanceOf(FixtureValidationError);
      expect((e as FixtureValidationError).issues).toContain('aggregateType must be a string');
    }
  });

  it('should report non-array events', () => {
    const fixture = { description: 'Test', aggregateType: 'Order', events: 'not-array' };
    try {
      validateFixture(fixture, 'test.json');
      throw new Error('Expected FixtureValidationError');
    } catch (e) {
      expect(e).toBeInstanceOf(FixtureValidationError);
      expect((e as FixtureValidationError).issues).toContain('events must be an array');
    }
  });

  it('should report event without type string', () => {
    const fixture = {
      description: 'Test',
      aggregateType: 'Order',
      events: [{ version: 'v1', data: {} }],
    };
    try {
      validateFixture(fixture, 'test.json');
      throw new Error('Expected FixtureValidationError');
    } catch (e) {
      expect(e).toBeInstanceOf(FixtureValidationError);
      expect((e as FixtureValidationError).issues).toContain('events[0].type must be a string');
    }
  });

  it('should report event without version string', () => {
    const fixture = {
      description: 'Test',
      aggregateType: 'Order',
      events: [{ type: 'OrderPlaced', data: {} }],
    };
    try {
      validateFixture(fixture, 'test.json');
      throw new Error('Expected FixtureValidationError');
    } catch (e) {
      expect(e).toBeInstanceOf(FixtureValidationError);
      expect((e as FixtureValidationError).issues).toContain('events[0].version must be a string');
    }
  });

  it('should report event without data object', () => {
    const fixture = {
      description: 'Test',
      aggregateType: 'Order',
      events: [{ type: 'OrderPlaced', version: 'v1' }],
    };
    try {
      validateFixture(fixture, 'test.json');
      throw new Error('Expected FixtureValidationError');
    } catch (e) {
      expect(e).toBeInstanceOf(FixtureValidationError);
      expect((e as FixtureValidationError).issues).toContain('events[0].data must be an object');
    }
  });

  it('should report multiple issues together', () => {
    const fixture = { events: 'not-array' };
    try {
      validateFixture(fixture, 'bad.json');
      throw new Error('Expected FixtureValidationError');
    } catch (e) {
      expect(e).toBeInstanceOf(FixtureValidationError);
      const err = e as FixtureValidationError;
      expect(err.issues.length).toBeGreaterThanOrEqual(3);
      expect(err.issues).toContain('description must be a string');
      expect(err.issues).toContain('aggregateType must be a string');
      expect(err.issues).toContain('events must be an array');
    }
  });

  it('should include filePath in error', () => {
    try {
      validateFixture(null, 'fixtures/broken.json');
      throw new Error('Expected FixtureValidationError');
    } catch (e) {
      expect(e).toBeInstanceOf(FixtureValidationError);
      const err = e as FixtureValidationError;
      expect(err.filePath).toBe('fixtures/broken.json');
      expect(err.message).toContain('fixtures/broken.json');
    }
  });
});
