/**
 * Tests for state assertion utilities (extracted helpers)
 * Verifies createDiff, deepEqual, partialMatch, formatDifferences
 */

import {
  createDiff,
  deepEqual,
  partialMatch,
  formatDifferences,
  StateDifference,
} from '../src/testing/replay/state-assertions';

describe('deepEqual', () => {
  it('should return true for identical primitives', () => {
    expect(deepEqual(1, 1)).toBe(true);
    expect(deepEqual('abc', 'abc')).toBe(true);
    expect(deepEqual(true, true)).toBe(true);
  });

  it('should return false for different primitives', () => {
    expect(deepEqual(1, 2)).toBe(false);
    expect(deepEqual('a', 'b')).toBe(false);
    expect(deepEqual(true, false)).toBe(false);
  });

  it('should handle null and undefined', () => {
    expect(deepEqual(null, null)).toBe(true);
    expect(deepEqual(undefined, undefined)).toBe(true);
    expect(deepEqual(null, undefined)).toBe(false);
    expect(deepEqual(null, 0)).toBe(false);
    expect(deepEqual(undefined, '')).toBe(false);
  });

  it('should return false for different types', () => {
    expect(deepEqual(1, '1')).toBe(false);
    expect(deepEqual(0, false)).toBe(false);
  });

  it('should compare arrays element by element', () => {
    expect(deepEqual([1, 2, 3], [1, 2, 3])).toBe(true);
    expect(deepEqual([1, 2], [1, 2, 3])).toBe(false);
    expect(deepEqual([1, 2, 3], [1, 2])).toBe(false);
    expect(deepEqual([1, 2], [1, 3])).toBe(false);
    expect(deepEqual([], [])).toBe(true);
  });

  it('should compare nested objects', () => {
    expect(deepEqual({ a: 1, b: { c: 2 } }, { a: 1, b: { c: 2 } })).toBe(true);
    expect(deepEqual({ a: 1, b: { c: 2 } }, { a: 1, b: { c: 3 } })).toBe(false);
    expect(deepEqual({ a: 1 }, { a: 1, b: 2 })).toBe(false);
  });

  it('should compare Date objects by time value', () => {
    const d1 = new Date('2026-01-01T00:00:00Z');
    const d2 = new Date('2026-01-01T00:00:00Z');
    const d3 = new Date('2026-06-15T00:00:00Z');

    expect(deepEqual(d1, d2)).toBe(true);
    expect(deepEqual(d1, d3)).toBe(false);
  });

  it('should handle same reference', () => {
    const obj = { a: 1 };
    expect(deepEqual(obj, obj)).toBe(true);
  });
});

describe('partialMatch', () => {
  it('should match when expected is a subset of actual', () => {
    expect(partialMatch({ a: 1 }, { a: 1, b: 2 })).toBe(true);
  });

  it('should fail when expected field differs', () => {
    expect(partialMatch({ a: 1 }, { a: 2, b: 2 })).toBe(false);
  });

  it('should match nested partial objects', () => {
    const expected = { user: { name: 'Alice' } };
    const actual = { user: { name: 'Alice', age: 30 }, active: true };
    expect(partialMatch(expected, actual)).toBe(true);
  });

  it('should fail on nested mismatch', () => {
    const expected = { user: { name: 'Alice' } };
    const actual = { user: { name: 'Bob', age: 30 } };
    expect(partialMatch(expected, actual)).toBe(false);
  });

  it('should handle arrays as prefix match', () => {
    expect(partialMatch([1, 2], [1, 2, 3])).toBe(true);
    expect(partialMatch([1, 2, 3], [1, 2])).toBe(false);
  });

  it('should handle null and undefined in expected', () => {
    expect(partialMatch(null, null)).toBe(true);
    expect(partialMatch(null, 'something')).toBe(false);
    expect(partialMatch(undefined, undefined)).toBe(true);
  });

  it('should return false when actual is null but expected is not', () => {
    expect(partialMatch({ a: 1 }, null)).toBe(false);
  });

  it('should compare dates', () => {
    const d1 = new Date('2026-03-01');
    const d2 = new Date('2026-03-01');
    expect(partialMatch(d1, d2)).toBe(true);
  });

  it('should handle primitives', () => {
    expect(partialMatch(42, 42)).toBe(true);
    expect(partialMatch(42, 43)).toBe(false);
  });
});

describe('createDiff', () => {
  it('should return empty array for identical values', () => {
    expect(createDiff(1, 1)).toEqual([]);
    expect(createDiff('hello', 'hello')).toEqual([]);
    expect(createDiff(null, null)).toEqual([]);
  });

  it('should report primitive mismatch', () => {
    const diffs = createDiff(1, 2, 'count');
    expect(diffs).toEqual([{ path: 'count', expected: 1, actual: 2 }]);
  });

  it('should use "root" path for top-level mismatch with empty path', () => {
    const diffs = createDiff(1, 2);
    expect(diffs).toEqual([{ path: 'root', expected: 1, actual: 2 }]);
  });

  it('should handle null vs non-null', () => {
    const diffs = createDiff(null, 'value', 'field');
    expect(diffs).toHaveLength(1);
    expect(diffs[0]).toEqual({ path: 'field', expected: null, actual: 'value' });
  });

  it('should handle undefined vs value', () => {
    const diffs = createDiff(undefined, 42, 'x');
    expect(diffs).toHaveLength(1);
    expect(diffs[0].expected).toBeUndefined();
    expect(diffs[0].actual).toBe(42);
  });

  it('should diff arrays: missing elements', () => {
    const diffs = createDiff([1, 2, 3], [1], 'items');
    // Element at index 1 and 2 missing from actual
    expect(diffs).toHaveLength(2);
    expect(diffs[0].path).toBe('items[1]');
    expect(diffs[0].actual).toBeUndefined();
    expect(diffs[1].path).toBe('items[2]');
  });

  it('should diff arrays: element value differences', () => {
    const diffs = createDiff([1, 99, 3], [1, 2, 3], 'arr');
    expect(diffs).toHaveLength(1);
    expect(diffs[0]).toEqual({ path: 'arr[1]', expected: 99, actual: 2 });
  });

  it('should diff nested objects', () => {
    const expected = { user: { name: 'Alice', age: 30 } };
    const actual = { user: { name: 'Bob', age: 30 } };
    const diffs = createDiff(expected, actual);
    expect(diffs).toHaveLength(1);
    expect(diffs[0]).toEqual({ path: 'user.name', expected: 'Alice', actual: 'Bob' });
  });

  it('should diff Date objects', () => {
    const d1 = new Date('2026-01-01');
    const d2 = new Date('2026-06-01');
    const diffs = createDiff(d1, d2, 'createdAt');
    expect(diffs).toHaveLength(1);
    expect(diffs[0].path).toBe('createdAt');
  });

  it('should return empty for equal Date objects', () => {
    const d1 = new Date('2026-01-01');
    const d2 = new Date('2026-01-01');
    expect(createDiff(d1, d2, 'date')).toEqual([]);
  });

  it('should report type mismatch', () => {
    const diffs = createDiff('hello', 42, 'field');
    expect(diffs).toHaveLength(1);
    expect(diffs[0]).toEqual({ path: 'field', expected: 'hello', actual: 42 });
  });

  it('should report missing object keys', () => {
    const expected = { a: 1, b: 2 };
    const actual = { a: 1 };
    const diffs = createDiff(expected, actual);
    expect(diffs).toHaveLength(1);
    expect(diffs[0]).toEqual({ path: 'b', expected: 2, actual: undefined });
  });
});

describe('formatDifferences', () => {
  it('should return "No differences found" for empty array', () => {
    expect(formatDifferences([])).toBe('No differences found');
  });

  it('should produce readable output for differences', () => {
    const diffs: StateDifference[] = [{ path: 'name', expected: 'Alice', actual: 'Bob' }];
    const output = formatDifferences(diffs);
    expect(output).toContain('Found 1 difference(s)');
    expect(output).toContain('name:');
    expect(output).toContain('expected: "Alice"');
    expect(output).toContain('actual:   "Bob"');
  });

  it('should list multiple differences', () => {
    const diffs: StateDifference[] = [
      { path: 'a', expected: 1, actual: 2 },
      { path: 'b', expected: 'x', actual: 'y' },
    ];
    const output = formatDifferences(diffs);
    expect(output).toContain('Found 2 difference(s)');
    expect(output).toContain('a:');
    expect(output).toContain('b:');
  });
});
