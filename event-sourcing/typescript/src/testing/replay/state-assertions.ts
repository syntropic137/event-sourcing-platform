/**
 * State assertion utilities for comparing expected and actual aggregate states
 */

/**
 * A single difference between expected and actual state
 */
export interface StateDifference {
  /** Path to the differing field (e.g., "order.items[0].quantity") */
  path: string;

  /** Expected value */
  expected: unknown;

  /** Actual value */
  actual: unknown;
}

/**
 * Type for partial matching (only specified fields are checked)
 */
export type DeepPartialMatch<T> = {
  [P in keyof T]?: T[P] extends object ? DeepPartialMatch<T[P]> : T[P];
};

/**
 * Check if two values are deeply equal
 */
export function deepEqual(a: unknown, b: unknown): boolean {
  // Same reference or both primitive and equal
  if (a === b) return true;

  // Handle null/undefined
  if (a == null || b == null) return a === b;

  // Handle different types
  if (typeof a !== typeof b) return false;

  // Handle arrays
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return false;
    return a.every((item, index) => deepEqual(item, b[index]));
  }

  // Handle dates
  if (a instanceof Date && b instanceof Date) {
    return a.getTime() === b.getTime();
  }

  // Handle objects
  if (typeof a === 'object' && typeof b === 'object') {
    const keysA = Object.keys(a as object);
    const keysB = Object.keys(b as object);

    if (keysA.length !== keysB.length) return false;

    return keysA.every((key) =>
      deepEqual((a as Record<string, unknown>)[key], (b as Record<string, unknown>)[key])
    );
  }

  return false;
}

/**
 * Check if actual state contains all fields from expected state (partial match)
 */
export function partialMatch(expected: unknown, actual: unknown): boolean {
  // Same reference or both primitive and equal
  if (expected === actual) return true;

  // Handle null/undefined
  if (expected == null) return expected === actual;
  if (actual == null) return false;

  // Handle different types
  if (typeof expected !== typeof actual) return false;

  // Handle arrays - expected elements must match corresponding actual elements
  // This is a "prefix match" for partial testing: [a, b] matches [a, b, c]
  // The length check ensures expected isn't longer than actual
  if (Array.isArray(expected) && Array.isArray(actual)) {
    if (expected.length > actual.length) return false;
    // Each expected element at index i must match actual element at index i
    return expected.every((item, index) => partialMatch(item, actual[index]));
  }

  // Handle dates
  if (expected instanceof Date && actual instanceof Date) {
    return expected.getTime() === actual.getTime();
  }

  // Handle objects - expected keys must exist and match
  if (typeof expected === 'object' && typeof actual === 'object') {
    const expectedObj = expected as Record<string, unknown>;
    const actualObj = actual as Record<string, unknown>;

    return Object.keys(expectedObj).every((key) => partialMatch(expectedObj[key], actualObj[key]));
  }

  return false;
}

/**
 * Diff two arrays element-by-element (only checks expected indices)
 */
function diffArrays(expected: unknown[], actual: unknown[], path: string): StateDifference[] {
  const differences: StateDifference[] = [];
  expected.forEach((item, index) => {
    const itemPath = path ? `${path}[${index}]` : `[${index}]`;
    if (index >= actual.length) {
      differences.push({ path: itemPath, expected: item, actual: undefined });
    } else {
      differences.push(...createDiff(item, actual[index], itemPath));
    }
  });
  return differences;
}

/**
 * Diff two objects by expected keys (partial match)
 */
function diffObjects(
  expected: Record<string, unknown>,
  actual: Record<string, unknown>,
  path: string
): StateDifference[] {
  const differences: StateDifference[] = [];
  for (const key of Object.keys(expected)) {
    const keyPath = path ? `${path}.${key}` : key;
    if (!(key in actual)) {
      differences.push({ path: keyPath, expected: expected[key], actual: undefined });
    } else {
      differences.push(...createDiff(expected[key], actual[key], keyPath));
    }
  }
  return differences;
}

/**
 * Diff two Date instances by time value
 */
function diffDates(expected: Date, actual: Date, path: string): StateDifference[] {
  return expected.getTime() === actual.getTime()
    ? []
    : [{ path: path || 'root', expected, actual }];
}

/**
 * Build a single-item mismatch diff
 */
function mismatch(expected: unknown, actual: unknown, path: string): StateDifference[] {
  return expected === actual ? [] : [{ path: path || 'root', expected, actual }];
}

/**
 * Create a diff between expected and actual values
 */
export function createDiff(
  expected: unknown,
  actual: unknown,
  path: string = ''
): StateDifference[] {
  if (expected === actual) return [];

  if (expected == null || actual == null || typeof expected !== typeof actual) {
    return mismatch(expected, actual, path);
  }

  if (Array.isArray(expected) && Array.isArray(actual)) {
    return diffArrays(expected, actual, path);
  }

  if (expected instanceof Date && actual instanceof Date) {
    return diffDates(expected, actual, path);
  }

  if (typeof expected === 'object' && typeof actual === 'object') {
    return diffObjects(
      expected as Record<string, unknown>,
      actual as Record<string, unknown>,
      path
    );
  }

  return mismatch(expected, actual, path);
}

/**
 * Format differences as a readable string
 */
export function formatDifferences(differences: StateDifference[]): string {
  if (differences.length === 0) {
    return 'No differences found';
  }

  const lines = differences.map((diff) => {
    const expectedStr = JSON.stringify(diff.expected);
    const actualStr = JSON.stringify(diff.actual);
    return `  ${diff.path}:\n    expected: ${expectedStr}\n    actual:   ${actualStr}`;
  });

  return `Found ${differences.length} difference(s):\n${lines.join('\n')}`;
}

/**
 * Assert that actual state matches expected state (throws on mismatch)
 */
export function assertStateMatches(
  expected: Record<string, unknown>,
  actual: Record<string, unknown>
): void {
  const differences = createDiff(expected, actual);

  if (differences.length > 0) {
    throw new StateAssertionError(differences);
  }
}

/**
 * Error thrown when state assertion fails
 */
export class StateAssertionError extends Error {
  constructor(public readonly differences: StateDifference[]) {
    super(formatDifferences(differences));
    this.name = 'StateAssertionError';
  }
}
