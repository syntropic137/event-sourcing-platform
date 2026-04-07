/**
 * Projection Ops - Production-ready projection infrastructure
 *
 * This module provides:
 * - Checkpointed projections with reliable event processing
 * - Checkpoint stores (PostgreSQL + in-memory)
 * - Failure handling with retry and dead letter queue
 * - Subscription coordinator for multi-projection management
 * - Health checks for monitoring
 *
 * @example
 * ```typescript
 * import {
 *   CheckpointedProjection,
 *   SubscriptionCoordinator,
 *   PostgresCheckpointStore,
 *   MemoryFailedEventStore,
 *   RetryPolicy,
 *   ProjectionHealthChecker,
 * } from '@syntropic137/event-sourcing-typescript/projections';
 *
 * // Create a projection
 * class OrderSummaryProjection extends CheckpointedProjection {
 *   getName() { return 'order-summary'; }
 *   getVersion() { return 1; }
 *   getSubscribedEventTypes() { return new Set(['OrderCreated', 'OrderShipped']); }
 *
 *   protected async processEvent(envelope, checkpointStore) {
 *     // Handle event...
 *     await this.saveCheckpoint(checkpointStore, envelope.metadata.globalNonce);
 *     return ProjectionResult.SUCCESS;
 *   }
 * }
 *
 * // Set up coordinator
 * const coordinator = new SubscriptionCoordinator({
 *   eventStore,
 *   checkpointStore: new PostgresCheckpointStore({ client: pool }),
 *   projections: [new OrderSummaryProjection()],
 *   failedEventStore: new MemoryFailedEventStore(),
 *   defaultRetryPolicy: RetryPolicy.exponentialBackoff({ maxRetries: 3 }),
 * });
 *
 * await coordinator.start();
 * ```
 */

// ============================================================================
// TYPES
// ============================================================================

export { ProjectionResult, DEFAULT_RETRY_POLICY } from './types';

export type {
  ProjectionCheckpoint,
  FailedEvent,
  FailedEventStatus,
  ProjectionHealth,
  RetryPolicyConfig,
} from './types';

// ============================================================================
// CHECKPOINTED PROJECTION
// ============================================================================

export {
  CheckpointedProjection,
  InMemoryProjection,
} from './checkpointed-projection';

// ============================================================================
// CHECKPOINT STORES
// ============================================================================

export { MemoryCheckpointStore } from './checkpoint/checkpoint-store';

export type { ProjectionCheckpointStore } from './checkpoint/checkpoint-store';

export { PostgresCheckpointStore } from './checkpoint/postgres-checkpoint-store';

export type {
  PostgresCheckpointStoreOptions,
  PostgresClient,
} from './checkpoint/postgres-checkpoint-store';

// ============================================================================
// FAILURE HANDLING
// ============================================================================

export { MemoryFailedEventStore } from './failure/failed-event-store';

export type { FailedEventStore } from './failure/failed-event-store';

// Note: generateFailedEventId is intentionally not exported as it's an internal utility

export { RetryPolicy } from './failure/retry-policy';

// Note: sleep is intentionally not exported as it's an internal utility

export { ProjectionErrorHandler } from './failure/error-handler';

export type {
  ErrorHandlerConfig,
  ErrorHandlerCallbacks,
  ErrorHandleResult,
} from './failure/error-handler';

// ============================================================================
// SUBSCRIPTION COORDINATOR
// ============================================================================

export { SubscriptionCoordinator } from './subscription-coordinator';

export type {
  SubscriptionCoordinatorConfig,
  EventStoreSubscription,
  CoordinatorLogger,
  CoordinatorState,
} from './subscription-coordinator';

// ============================================================================
// HEALTH CHECKS
// ============================================================================

export {
  ProjectionHealthChecker,
  DEFAULT_HEALTH_THRESHOLDS,
  createHealthCheckResponse,
} from './health/projection-health';

export type {
  HealthCheckerConfig,
  HealthThresholds,
  PositionProvider,
} from './health/projection-health';
