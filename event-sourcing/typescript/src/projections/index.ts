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
 * } from '@event-sourcing-platform/typescript/projections';
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

export {
  ProjectionResult,
  ProjectionCheckpoint,
  FailedEvent,
  FailedEventStatus,
  ProjectionHealth,
  RetryPolicyConfig,
  DEFAULT_RETRY_POLICY,
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

export {
  ProjectionCheckpointStore,
  MemoryCheckpointStore,
} from './checkpoint/checkpoint-store';

export {
  PostgresCheckpointStore,
  PostgresCheckpointStoreOptions,
  PostgresClient,
} from './checkpoint/postgres-checkpoint-store';

// ============================================================================
// FAILURE HANDLING
// ============================================================================

export {
  FailedEventStore,
  MemoryFailedEventStore,
  generateFailedEventId,
} from './failure/failed-event-store';

export {
  RetryPolicy,
  sleep,
} from './failure/retry-policy';

export {
  ProjectionErrorHandler,
  ErrorHandlerConfig,
  ErrorHandlerCallbacks,
  ErrorHandleResult,
} from './failure/error-handler';

// ============================================================================
// SUBSCRIPTION COORDINATOR
// ============================================================================

export {
  SubscriptionCoordinator,
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
  HealthCheckerConfig,
  HealthThresholds,
  DEFAULT_HEALTH_THRESHOLDS,
  PositionProvider,
  createHealthCheckResponse,
} from './health/projection-health';
