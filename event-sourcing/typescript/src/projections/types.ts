/**
 * Core types for projection operations
 */

/**
 * Result of processing an event in a projection
 */
export enum ProjectionResult {
  /** Event processed successfully, advance checkpoint */
  SUCCESS = 'success',

  /** Event not relevant to this projection, advance checkpoint */
  SKIP = 'skip',

  /** Transient error, should retry */
  RETRY = 'retry',

  /** Permanent error, send to DLQ */
  FAILURE = 'failure',
}

/**
 * Checkpoint data for a projection
 */
export interface ProjectionCheckpoint {
  /** Unique projection name */
  projectionName: string;

  /** Last successfully processed global position */
  globalPosition: number;

  /** When this checkpoint was last updated */
  updatedAt: Date;

  /** Projection schema version (for rebuild detection) */
  version: number;
}

/**
 * A failed event record for the dead letter queue
 */
export interface FailedEvent {
  /** Unique ID for this failure record */
  id: string;

  /** Which projection failed */
  projectionName: string;

  /** The failed event's ID */
  eventId: string;

  /** The failed event's type */
  eventType: string;

  /** Position in global stream */
  globalNonce: number;

  /** The aggregate that produced the event */
  aggregateId: string;

  /** Type of aggregate */
  aggregateType: string;

  /** Serialized event data */
  payload: string;

  /** Error message */
  errorMessage: string;

  /** Stack trace */
  errorStack?: string;

  /** How many times we tried */
  attemptCount: number;

  /** When it first failed */
  firstFailedAt: Date;

  /** Most recent failure */
  lastFailedAt: Date;

  /** Current status */
  status: FailedEventStatus;

  /** Reason if ignored */
  ignoreReason?: string;
}

/**
 * Status of a failed event
 */
export type FailedEventStatus = 'pending' | 'reprocessing' | 'resolved' | 'ignored';

/**
 * Health status of a projection
 */
export interface ProjectionHealth {
  /** Projection name */
  projectionName: string;

  /** Overall health status */
  status: 'healthy' | 'degraded' | 'unhealthy';

  /** Last processed global position */
  lastProcessedPosition: number;

  /** Current head position in event store */
  currentHeadPosition: number;

  /** Number of events behind */
  lag: number;

  /** When last event was processed */
  lastProcessedAt: Date;

  /** Number of events in DLQ */
  failedEventCount: number;

  /** Recent error messages */
  recentErrors: string[];
}

/**
 * Configuration for retry policy
 */
export interface RetryPolicyConfig {
  /** Maximum number of retry attempts */
  maxRetries: number;

  /** Initial delay in milliseconds */
  initialDelayMs: number;

  /** Maximum delay in milliseconds */
  maxDelayMs: number;

  /** Multiplier for exponential backoff */
  backoffMultiplier: number;

  /** Optional: specific error patterns to retry */
  retryablePatterns?: string[];
}

/**
 * Default retry policy configuration
 */
export const DEFAULT_RETRY_POLICY: RetryPolicyConfig = {
  maxRetries: 3,
  initialDelayMs: 100,
  maxDelayMs: 30000,
  backoffMultiplier: 2,
};
