/**
 * ES Test Kit - Testing utilities for Event Sourcing applications
 *
 * This module provides:
 * - Given-When-Then scenario testing (test command → event flow)
 * - Golden replay testing (verify aggregate state from known events)
 * - Invariant testing (verify business rules hold after each event)
 * - Projection testing (verify projection correctness and determinism)
 *
 * @example
 * ```typescript
 * import {
 *   scenario,
 *   loadFixture,
 *   ReplayTester,
 *   InvariantChecker,
 *   ProjectionTester,
 * } from '@syntropic137/event-sourcing-typescript/testing';
 *
 * // Given-When-Then scenario test
 * scenario(OrderAggregate)
 *   .given([new CartCreatedEvent('order-1')])
 *   .when(new AddItemCommand('order-1', 'item-1', 29.99))
 *   .expectEvents([new ItemAddedEvent('order-1', 'item-1', 29.99)]);
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
// SCENARIO TESTING (Given-When-Then)
// ============================================================================

export { scenario, AggregateScenario } from './scenario';
export { TestExecutor } from './scenario';
export { ResultValidator } from './scenario';
export { ScenarioAssertionError, ScenarioExecutionError } from './scenario';

// ============================================================================
// FIXTURES
// ============================================================================

export { validateFixture } from './fixtures/fixture-types';

export type {
  TestFixture,
  FixtureEvent,
  ExpectedState,
  LoadedFixture,
  LoadFixtureOptions,
  FixtureValidationError,
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

export { ReplayTester, createReplayTester } from './replay/replay-tester';

export type {
  ReplayResult,
  ReplayError,
  StateComparisonResult,
  StateDifference,
  ReplayTesterOptions,
  EventFactory,
} from './replay/replay-tester';

export {
  deepEqual,
  partialMatch,
  createDiff,
  formatDifferences,
  assertStateMatches,
  StateAssertionError,
} from './replay/state-assertions';

export type { DeepPartialMatch } from './replay/state-assertions';

// ============================================================================
// INVARIANT TESTING
// ============================================================================

export {
  INVARIANT_METADATA,
  Invariant,
  getInvariants,
  hasInvariants,
} from './invariants/invariant-decorator';

export type {
  InvariantMetadata,
  InvariantOptions,
  InvariantAwareConstructor,
} from './invariants/invariant-decorator';

export { InvariantChecker, createInvariantChecker } from './invariants/invariant-checker';

export type {
  InvariantCheckResult,
  InvariantSnapshot,
  InvariantVerificationResult,
  InvariantViolation,
  InvariantCheckerOptions,
} from './invariants/invariant-checker';

// ============================================================================
// PROJECTION TESTING
// ============================================================================

export { ProjectionTester, createProjectionTester } from './projections/projection-tester';

export type {
  TestableProjection,
  ProjectionTestResult,
  ProjectionTestError,
  DeterminismResult,
  ProjectionTesterOptions,
} from './projections/projection-tester';
