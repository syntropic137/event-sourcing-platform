/**
 * Type definitions for ES test fixtures
 *
 * Fixtures are JSON/YAML files that define event sequences for testing.
 * They enable golden replay testing - verifying that known event sequences
 * produce expected aggregate states.
 */

import { JsonObject, JsonValue } from '../../types/common';

/**
 * A single event in a test fixture
 */
export interface FixtureEvent {
  /** Event type name (e.g., "OrderPlaced") */
  type: string;

  /** Event schema version (e.g., "v1") */
  version: string;

  /** Event payload data */
  data: JsonObject;

  /** Optional metadata overrides */
  metadata?: {
    eventId?: string;
    timestamp?: string;
    correlationId?: string;
    causationId?: string;
    actorId?: string;
    tenantId?: string;
  };
}

/**
 * Expected state after replaying events
 */
export interface ExpectedState {
  /** Partial state to match (deep equality on specified fields) */
  [key: string]: JsonValue;
}

/**
 * A complete test fixture definition
 */
export interface TestFixture {
  /** Human-readable description of what this fixture tests */
  description: string;

  /** The aggregate type being tested */
  aggregateType: string;

  /** The aggregate ID to use (optional, defaults to generated) */
  aggregateId?: string;

  /** Sequence of events to replay */
  events: FixtureEvent[];

  /** Expected state after all events are applied (optional) */
  expectedState?: ExpectedState;

  /** Expected version after all events (optional) */
  expectedVersion?: number;

  /** Tags for filtering/organizing fixtures */
  tags?: string[];
}

/**
 * Result of loading a fixture file
 */
export interface LoadedFixture extends TestFixture {
  /** Path to the fixture file */
  filePath: string;

  /** File format (json or yaml) */
  format: 'json' | 'yaml';
}

/**
 * Options for loading fixtures
 */
export interface LoadFixtureOptions {
  /** Base directory for relative paths */
  baseDir?: string;

  /** Whether to validate fixture structure */
  validate?: boolean;
}

/**
 * Fixture validation error
 */
export class FixtureValidationError extends Error {
  constructor(
    public readonly filePath: string,
    public readonly issues: string[]
  ) {
    super(`Invalid fixture ${filePath}: ${issues.join(', ')}`);
    this.name = 'FixtureValidationError';
  }
}

/**
 * Validate a single fixture event entry
 */
function validateFixtureEvent(event: unknown, index: number): string[] {
  if (!event || typeof event !== 'object') {
    return [`events[${index}] must be an object`];
  }
  const e = event as Record<string, unknown>;
  const issues: string[] = [];
  if (typeof e.type !== 'string') {
    issues.push(`events[${index}].type must be a string`);
  }
  if (typeof e.version !== 'string') {
    issues.push(`events[${index}].version must be a string`);
  }
  if (!e.data || typeof e.data !== 'object') {
    issues.push(`events[${index}].data must be an object`);
  }
  return issues;
}

/**
 * Validates a fixture structure
 */
export function validateFixture(fixture: unknown, filePath: string): TestFixture {
  if (!fixture || typeof fixture !== 'object') {
    throw new FixtureValidationError(filePath, ['Fixture must be an object']);
  }

  const f = fixture as Record<string, unknown>;
  const issues: string[] = [];

  if (typeof f.description !== 'string') {
    issues.push('description must be a string');
  }
  if (typeof f.aggregateType !== 'string') {
    issues.push('aggregateType must be a string');
  }

  if (!Array.isArray(f.events)) {
    issues.push('events must be an array');
  } else {
    f.events.forEach((event, index) => {
      issues.push(...validateFixtureEvent(event, index));
    });
  }

  if (issues.length > 0) {
    throw new FixtureValidationError(filePath, issues);
  }
  return fixture as TestFixture;
}
