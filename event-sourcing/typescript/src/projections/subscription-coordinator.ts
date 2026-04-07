/**
 * SubscriptionCoordinator - Manages event subscription across multiple projections
 *
 * Coordinates reading from the event store and dispatching events to projections
 * with proper checkpoint management and error handling.
 */

import { DomainEvent, EventEnvelope } from '../core/event';
import { ProjectionResult } from './types';
import { CheckpointedProjection } from './checkpointed-projection';
import { ProjectionCheckpointStore } from './checkpoint/checkpoint-store';
import { ProjectionErrorHandler } from './failure/error-handler';
import { FailedEventStore } from './failure/failed-event-store';
import { RetryPolicy } from './failure/retry-policy';

/**
 * Interface for event store subscription
 */
export interface EventStoreSubscription {
  /**
   * Subscribe to events from a given position
   * @param fromGlobalNonce - Starting position (inclusive)
   * @returns Async iterator of event envelopes
   */
  subscribe(fromGlobalNonce: number): AsyncIterable<EventEnvelope<DomainEvent>>;

  /**
   * Get the current head position
   */
  getCurrentPosition(): Promise<number>;
}

/**
 * Logger interface for coordinator
 */
export interface CoordinatorLogger {
  debug(message: string, context?: Record<string, unknown>): void;
  info(message: string, context?: Record<string, unknown>): void;
  warn(message: string, context?: Record<string, unknown>): void;
  error(message: string, context?: Record<string, unknown>): void;
}

/**
 * Default console logger
 */
const defaultLogger: CoordinatorLogger = {
  debug: (msg, ctx) => console.debug(`[Coordinator] ${msg}`, ctx ?? ''),
  info: (msg, ctx) => console.info(`[Coordinator] ${msg}`, ctx ?? ''),
  warn: (msg, ctx) => console.warn(`[Coordinator] ${msg}`, ctx ?? ''),
  error: (msg, ctx) => console.error(`[Coordinator] ${msg}`, ctx ?? ''),
};

/**
 * Configuration for SubscriptionCoordinator
 */
export interface SubscriptionCoordinatorConfig {
  /** Event store subscription */
  eventStore: EventStoreSubscription;

  /** Checkpoint store */
  checkpointStore: ProjectionCheckpointStore;

  /** Projections to coordinate */
  projections: CheckpointedProjection[];

  /** Failed event store (DLQ) */
  failedEventStore: FailedEventStore;

  /** Default retry policy */
  defaultRetryPolicy?: RetryPolicy;

  /** Per-projection retry policy overrides */
  retryPolicyOverrides?: Map<string, RetryPolicy>;

  /** Logger */
  logger?: CoordinatorLogger;

  /** Callbacks for monitoring */
  callbacks?: {
    onEventProcessed?: (
      projectionName: string,
      envelope: EventEnvelope<DomainEvent>,
      result: ProjectionResult
    ) => void;
    onError?: (projectionName: string, envelope: EventEnvelope<DomainEvent>, error: Error) => void;
    onBatchComplete?: (batchSize: number, projectionResults: Map<string, ProjectionResult>) => void;
  };
}

/**
 * Coordinator state
 */
export type CoordinatorState = 'stopped' | 'starting' | 'running' | 'stopping';

/**
 * SubscriptionCoordinator - Manages event distribution to projections
 *
 * @example
 * ```typescript
 * const coordinator = new SubscriptionCoordinator({
 *   eventStore,
 *   checkpointStore,
 *   projections: [orderSummary, inventoryLevels],
 *   failedEventStore: memoryFailedStore,
 *   defaultRetryPolicy: RetryPolicy.exponentialBackoff({ maxRetries: 3 }),
 * });
 *
 * await coordinator.start();
 * // ... later
 * await coordinator.stop();
 * ```
 */
export class SubscriptionCoordinator {
  private readonly eventStore: EventStoreSubscription;
  private readonly checkpointStore: ProjectionCheckpointStore;
  private readonly projections: Map<string, CheckpointedProjection>;
  private readonly errorHandlers: Map<string, ProjectionErrorHandler>;
  private readonly logger: CoordinatorLogger;
  private readonly callbacks: SubscriptionCoordinatorConfig['callbacks'];

  private state: CoordinatorState = 'stopped';
  private abortController: AbortController | null = null;

  constructor(config: SubscriptionCoordinatorConfig) {
    this.eventStore = config.eventStore;
    this.checkpointStore = config.checkpointStore;
    this.logger = config.logger ?? defaultLogger;
    this.callbacks = config.callbacks;

    // Index projections by name
    this.projections = new Map();
    for (const projection of config.projections) {
      this.projections.set(projection.getName(), projection);
    }

    // Create error handlers for each projection
    this.errorHandlers = new Map();
    const defaultRetryPolicy = config.defaultRetryPolicy ?? RetryPolicy.exponentialBackoff();

    for (const projection of config.projections) {
      const name = projection.getName();
      const retryPolicy = config.retryPolicyOverrides?.get(name) ?? defaultRetryPolicy;

      this.errorHandlers.set(
        name,
        new ProjectionErrorHandler({
          retryPolicy,
          failedEventStore: config.failedEventStore,
          callbacks: {
            onRetry: (envelope, attempt, error, delayMs) => {
              this.logger.warn('Retrying event', {
                projection: name,
                eventId: envelope.metadata.eventId,
                attempt,
                error: error.message,
                delayMs,
              });
            },
            onDLQ: (envelope, error) => {
              this.logger.error('Event sent to DLQ', {
                projection: name,
                eventId: envelope.metadata.eventId,
                error: error.message,
              });
              this.callbacks?.onError?.(name, envelope, error);
            },
            onSuccess: (envelope) => {
              this.logger.debug('Event processed successfully', {
                projection: name,
                eventId: envelope.metadata.eventId,
              });
            },
          },
        })
      );
    }
  }

  /**
   * Get the current coordinator state
   */
  getState(): CoordinatorState {
    return this.state;
  }

  /**
   * Start the coordinator
   */
  async start(): Promise<void> {
    if (this.state !== 'stopped') {
      throw new Error(`Cannot start coordinator in state: ${this.state}`);
    }

    this.state = 'starting';
    this.abortController = new AbortController();

    // Initialize all projections
    for (const projection of this.projections.values()) {
      await projection.initialize();
    }

    // Get minimum position across all projections
    const minPosition = await this.getMinimumPosition();

    this.logger.info('Starting subscription coordinator', {
      projectionCount: this.projections.size,
      fromPosition: minPosition,
    });

    this.state = 'running';

    // Start processing events
    try {
      await this.processEvents(minPosition);
    } catch (error) {
      if (this.state === 'running') {
        this.logger.error('Coordinator error', { error: (error as Error).message });
        throw error;
      }
      // If stopping, the error is expected (abort)
    }
  }

  /**
   * Stop the coordinator
   */
  async stop(): Promise<void> {
    if (this.state !== 'running') {
      return;
    }

    this.state = 'stopping';
    this.abortController?.abort();

    // Shutdown all projections
    for (const projection of this.projections.values()) {
      await projection.shutdown();
    }

    this.state = 'stopped';
    this.logger.info('Coordinator stopped');
  }

  /**
   * Get the minimum checkpoint position across all projections
   */
  private async getMinimumPosition(): Promise<number> {
    let minPosition = Number.MAX_SAFE_INTEGER;

    for (const projection of this.projections.values()) {
      const checkpoint = await this.checkpointStore.getCheckpoint(projection.getName());
      if (checkpoint === null) {
        // Projection has no checkpoint - start from beginning
        return 0;
      }
      minPosition = Math.min(minPosition, checkpoint.globalPosition);
    }

    // Start from next position after minimum checkpoint
    return minPosition === Number.MAX_SAFE_INTEGER ? 0 : minPosition + 1;
  }

  /**
   * Process events from the event store
   */
  private async processEvents(fromPosition: number): Promise<void> {
    for await (const envelope of this.eventStore.subscribe(fromPosition)) {
      // Check if we should stop
      if (this.abortController?.signal.aborted) {
        break;
      }

      await this.dispatchToProjections(envelope);
    }
  }

  /**
   * Dispatch an event to all relevant projections
   */
  private async dispatchToProjections(envelope: EventEnvelope<DomainEvent>): Promise<void> {
    const results = new Map<string, ProjectionResult>();

    for (const [name, projection] of this.projections) {
      const result = await this.dispatchToProjection(envelope, name, projection);
      results.set(name, result);
    }

    this.callbacks?.onBatchComplete?.(1, results);
  }

  /**
   * Dispatch an event to a single projection with error handling
   */
  private async dispatchToProjection(
    envelope: EventEnvelope<DomainEvent>,
    name: string,
    projection: CheckpointedProjection
  ): Promise<ProjectionResult> {
    const errorHandler = this.errorHandlers.get(name)!;

    const handleResult = await errorHandler.executeWithRetry(envelope, name, async () => {
      return projection.handleEvent(envelope, this.checkpointStore);
    });

    this.callbacks?.onEventProcessed?.(name, envelope, handleResult.result);
    return handleResult.result;
  }

  /**
   * Rebuild a specific projection from scratch
   *
   * IMPORTANT: If the coordinator is running, this method will stop it,
   * perform the rebuild, and restart it. This ensures the projection
   * replays events from position 0.
   *
   * @param projectionName - Name of the projection to rebuild
   * @throws Error if projection is not found
   */
  async rebuildProjection(projectionName: string): Promise<void> {
    const projection = this.projections.get(projectionName);
    if (!projection) {
      throw new Error(`Projection not found: ${projectionName}`);
    }

    const wasRunning = this.state === 'running';

    this.logger.info('Rebuilding projection', {
      projection: projectionName,
      wasRunning,
    });

    // If running, stop first to ensure clean restart from new position
    if (wasRunning) {
      this.logger.info('Stopping coordinator for rebuild');
      await this.stop();
    }

    // Delete checkpoint - this ensures we start from position 0
    await this.checkpointStore.deleteCheckpoint(projectionName);

    // Clear projection data
    await projection.clearData();

    this.logger.info('Projection data cleared', { projection: projectionName });

    // If was running, restart coordinator
    // It will now start from the new minimum position (0 for this projection)
    if (wasRunning) {
      this.logger.info('Restarting coordinator after rebuild');
      await this.start();
    }

    this.logger.info('Projection rebuild complete', { projection: projectionName });
  }

  /**
   * Get list of projection names
   */
  getProjectionNames(): string[] {
    return Array.from(this.projections.keys());
  }
}
