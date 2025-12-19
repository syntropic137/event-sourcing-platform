/**
 * Error handler for projection failures
 */

import { DomainEvent, EventEnvelope } from '../../core/event';
import { FailedEvent, ProjectionResult } from '../types';
import { FailedEventStore, generateFailedEventId } from './failed-event-store';
import { RetryPolicy, sleep } from './retry-policy';

/**
 * Callbacks for error handling events
 */
export interface ErrorHandlerCallbacks {
  /** Called when an event is being retried */
  onRetry?: (
    envelope: EventEnvelope<DomainEvent>,
    attempt: number,
    error: Error,
    delayMs: number
  ) => void;

  /** Called when an event is sent to DLQ */
  onDLQ?: (envelope: EventEnvelope<DomainEvent>, error: Error) => void;

  /** Called when an event is successfully processed */
  onSuccess?: (envelope: EventEnvelope<DomainEvent>) => void;

  /** Called to check if an error is retryable (overrides default) */
  isRetryable?: (error: Error) => boolean;
}

/**
 * Configuration for error handler
 */
export interface ErrorHandlerConfig {
  /** Retry policy */
  retryPolicy: RetryPolicy;

  /** Failed event store (DLQ) */
  failedEventStore: FailedEventStore;

  /** Optional callbacks */
  callbacks?: ErrorHandlerCallbacks;
}

/**
 * Result of error handling
 */
export interface ErrorHandleResult {
  /** Final result */
  result: ProjectionResult;

  /** Number of retries attempted */
  retryCount: number;

  /** Whether event was sent to DLQ */
  sentToDLQ: boolean;

  /** Error if failed */
  error?: Error;
}

/**
 * Handles errors during projection event processing
 *
 * Implements retry with backoff and dead letter queue.
 */
export class ProjectionErrorHandler {
  private readonly retryPolicy: RetryPolicy;
  private readonly failedEventStore: FailedEventStore;
  private readonly callbacks: ErrorHandlerCallbacks;

  constructor(config: ErrorHandlerConfig) {
    this.retryPolicy = config.retryPolicy;
    this.failedEventStore = config.failedEventStore;
    this.callbacks = config.callbacks ?? {};
  }

  /**
   * Handle an error from projection processing
   *
   * @param envelope - The event that failed
   * @param projectionName - Name of the projection
   * @param error - The error that occurred
   * @param attemptCount - Number of attempts so far (0 = first attempt)
   * @returns Whether to continue retrying
   */
  async handleError(
    envelope: EventEnvelope<DomainEvent>,
    projectionName: string,
    error: Error,
    attemptCount: number
  ): Promise<{ shouldRetry: boolean; delayMs: number }> {
    const isRetryable = this.callbacks.isRetryable
      ? this.callbacks.isRetryable(error)
      : this.retryPolicy.isRetryable(error);

    if (!isRetryable) {
      // Permanent error - send to DLQ immediately
      await this.sendToDLQ(envelope, projectionName, error, attemptCount);
      return { shouldRetry: false, delayMs: 0 };
    }

    if (!this.retryPolicy.shouldRetry(attemptCount)) {
      // Exhausted retries - send to DLQ
      await this.sendToDLQ(envelope, projectionName, error, attemptCount);
      return { shouldRetry: false, delayMs: 0 };
    }

    // Will retry
    const delayMs = this.retryPolicy.getDelay(attemptCount + 1);
    this.callbacks.onRetry?.(envelope, attemptCount + 1, error, delayMs);

    return { shouldRetry: true, delayMs };
  }

  /**
   * Send a failed event to the dead letter queue
   */
  async sendToDLQ(
    envelope: EventEnvelope<DomainEvent>,
    projectionName: string,
    error: Error,
    attemptCount: number
  ): Promise<void> {
    const failedEvent: FailedEvent = {
      id: generateFailedEventId(),
      projectionName,
      eventId: envelope.metadata.eventId,
      eventType: envelope.event.eventType,
      globalNonce: envelope.metadata.globalNonce ?? 0,
      aggregateId: envelope.metadata.aggregateId,
      aggregateType: envelope.metadata.aggregateType,
      payload: JSON.stringify(envelope.event.toJson()),
      errorMessage: error.message,
      errorStack: error.stack,
      attemptCount: attemptCount + 1,
      firstFailedAt: new Date(),
      lastFailedAt: new Date(),
      status: 'pending',
    };

    await this.failedEventStore.save(failedEvent);
    this.callbacks.onDLQ?.(envelope, error);
  }

  /**
   * Report successful processing
   */
  reportSuccess(envelope: EventEnvelope<DomainEvent>): void {
    this.callbacks.onSuccess?.(envelope);
  }

  /**
   * Execute a processing function with retry handling
   *
   * @param envelope - The event to process
   * @param projectionName - Name of the projection
   * @param processFn - The processing function
   * @returns Processing result
   */
  async executeWithRetry(
    envelope: EventEnvelope<DomainEvent>,
    projectionName: string,
    processFn: () => Promise<ProjectionResult>
  ): Promise<ErrorHandleResult> {
    let attemptCount = 0;
    let lastError: Error | undefined;

    while (true) {
      try {
        const result = await processFn();

        if (result === ProjectionResult.SUCCESS || result === ProjectionResult.SKIP) {
          this.reportSuccess(envelope);
          return {
            result,
            retryCount: attemptCount,
            sentToDLQ: false,
          };
        }

        if (result === ProjectionResult.FAILURE) {
          // Explicit failure - send to DLQ
          const error = new Error('Projection returned FAILURE result');
          await this.sendToDLQ(envelope, projectionName, error, attemptCount);
          return {
            result: ProjectionResult.FAILURE,
            retryCount: attemptCount,
            sentToDLQ: true,
            error,
          };
        }

        if (result === ProjectionResult.RETRY) {
          // Explicit retry request
          lastError = new Error('Projection returned RETRY result');
          const { shouldRetry, delayMs } = await this.handleError(
            envelope,
            projectionName,
            lastError,
            attemptCount
          );

          if (!shouldRetry) {
            return {
              result: ProjectionResult.FAILURE,
              retryCount: attemptCount,
              sentToDLQ: true,
              error: lastError,
            };
          }

          attemptCount++;
          await sleep(delayMs);
          continue;
        }
      } catch (error) {
        lastError = error as Error;
        const { shouldRetry, delayMs } = await this.handleError(
          envelope,
          projectionName,
          lastError,
          attemptCount
        );

        if (!shouldRetry) {
          return {
            result: ProjectionResult.FAILURE,
            retryCount: attemptCount,
            sentToDLQ: true,
            error: lastError,
          };
        }

        attemptCount++;
        await sleep(delayMs);
      }
    }
  }
}
