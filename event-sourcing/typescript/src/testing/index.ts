/**
 * ES Test Kit - Testing utilities for Event Sourcing applications
 *
 * This module provides:
 * - Golden replay testing (verify aggregate state from known events)
 * - Invariant testing (verify business rules hold after each event)
 * - Projection testing (verify projection correctness and determinism)
 *
 * @example
 * ```typescript
 * import {
 *   loadFixture,
 *   ReplayTester,
 *   InvariantChecker,
 *   ProjectionTester,
 * } from '@event-sourcing-platform/typescript/testing';
 *
 * // Golden replay test
 * const fixture = await loadFixture('./fixtures/order-lifecycle.json');
 * const tester = new ReplayTester(OrderAggregate);
 * const result = await tester.replayAndAssert(fixture);
 * expect(result.success).toBe(true);
 *
 * // Invariant test
 * const checker = new InvariantChecker(BankAccountAggregate);
 * const invariantResult = await checker.verifyAfterEachEvent(fixture.events);
 * expect(invariantResult.passed).toBe(true);
 *
 * // Projection test
 * const projTester = new ProjectionTester(new OrderSummaryProjection());
 * const determinism = await projTester.verifyDeterminism(events);
 * expect(determinism.isDeterministic).toBe(true);
 * ```
 */

// ============================================================================
// FIXTURES
// ============================================================================

export {
  // Types
  TestFixture,
  FixtureEvent,
  ExpectedState,
  LoadedFixture,
  LoadFixtureOptions,
  FixtureValidationError,
  // Functions
  validateFixture,
} from './fixtures/fixture-types';

export {
  loadFixture,
  loadFixturesFromDirectory,
  loadFixturesByTags,
  createFixture,
  saveFixture,
} from './fixtures/test-fixture';

// ============================================================================
// REPLAY TESTING
// ============================================================================

export {
  // Types
  ReplayResult,
  ReplayError,
  StateComparisonResult,
  StateDifference,
  ReplayTesterOptions,
  EventFactory,
  // Class
  ReplayTester,
  // Factory
  createReplayTester,
} from './replay/replay-tester';

export {
  // Types
  DeepPartialMatch,
  // Functions
  deepEqual,
  partialMatch,
  createDiff,
  formatDifferences,
  assertStateMatches,
  // Errors
  StateAssertionError,
} from './replay/state-assertions';

// ============================================================================
// INVARIANT TESTING
// ============================================================================

export {
  // Types
  InvariantMetadata,
  InvariantOptions,
  InvariantAwareConstructor,
  INVARIANT_METADATA,
  // Decorator
  Invariant,
  // Functions
  getInvariants,
  hasInvariants,
} from './invariants/invariant-decorator';

export {
  // Types
  InvariantCheckResult,
  InvariantSnapshot,
  InvariantVerificationResult,
  InvariantViolation,
  InvariantCheckerOptions,
  // Class
  InvariantChecker,
  // Factory
  createInvariantChecker,
} from './invariants/invariant-checker';

// ============================================================================
// PROJECTION TESTING
// ============================================================================

export {
  // Types
  TestableProjection,
  ProjectionTestResult,
  ProjectionTestError,
  DeterminismResult,
  ProjectionTesterOptions,
  // Class
  ProjectionTester,
  // Factory
  createProjectionTester,
} from './projections/projection-tester';
