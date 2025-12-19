/**
 * ES-Specific Metrics
 *
 * Pre-configured metrics for Event Sourcing operations.
 */

import { ES_METRICS, ES_METRIC_LABELS } from '../types';
import {
  counter,
  gauge,
  histogram,
  DEFAULT_LATENCY_BUCKETS,
} from './metrics';

// ============================================================================
// EVENT STORE METRICS
// ============================================================================

/**
 * Record that events were appended to a stream.
 */
export function recordEventsAppended(
  aggregateType: string,
  eventTypes: string[],
  tenantId?: string
): void {
  const eventsAppended = counter(
    ES_METRICS.EVENTS_APPENDED_TOTAL,
    'Total number of events appended',
    [ES_METRIC_LABELS.AGGREGATE_TYPE, ES_METRIC_LABELS.EVENT_TYPE, ES_METRIC_LABELS.TENANT_ID]
  );

  for (const eventType of eventTypes) {
    eventsAppended.inc({
      [ES_METRIC_LABELS.AGGREGATE_TYPE]: aggregateType,
      [ES_METRIC_LABELS.EVENT_TYPE]: eventType,
      [ES_METRIC_LABELS.TENANT_ID]: tenantId ?? '',
    });
  }
}

/**
 * Create a timer for measuring event append duration.
 */
export function startAppendTimer(aggregateType: string): () => number {
  const hist = histogram(
    ES_METRICS.EVENTS_APPEND_DURATION,
    'Time to append events to a stream',
    DEFAULT_LATENCY_BUCKETS,
    [ES_METRIC_LABELS.AGGREGATE_TYPE]
  );

  return hist.startTimer({
    [ES_METRIC_LABELS.AGGREGATE_TYPE]: aggregateType,
  });
}

/**
 * Create a timer for measuring aggregate load duration.
 */
export function startAggregateLoadTimer(aggregateType: string): () => number {
  const hist = histogram(
    ES_METRICS.AGGREGATE_LOAD_DURATION,
    'Time to load/rehydrate an aggregate',
    DEFAULT_LATENCY_BUCKETS,
    [ES_METRIC_LABELS.AGGREGATE_TYPE]
  );

  return hist.startTimer({
    [ES_METRIC_LABELS.AGGREGATE_TYPE]: aggregateType,
  });
}

/**
 * Create a timer for measuring command execution duration.
 */
export function startCommandTimer(
  commandType: string,
  aggregateType: string
): () => number {
  const hist = histogram(
    ES_METRICS.COMMAND_DURATION,
    'Time to execute a command',
    DEFAULT_LATENCY_BUCKETS,
    [ES_METRIC_LABELS.COMMAND_TYPE, ES_METRIC_LABELS.AGGREGATE_TYPE]
  );

  return hist.startTimer({
    [ES_METRIC_LABELS.COMMAND_TYPE]: commandType,
    [ES_METRIC_LABELS.AGGREGATE_TYPE]: aggregateType,
  });
}

// ============================================================================
// PROJECTION METRICS
// ============================================================================

/**
 * Record that a projection processed an event.
 */
export function recordProjectionEventProcessed(
  projectionName: string,
  eventType: string
): void {
  const eventsProcessed = counter(
    ES_METRICS.PROJECTION_EVENTS_PROCESSED,
    'Total events processed by projection',
    [ES_METRIC_LABELS.PROJECTION_NAME, ES_METRIC_LABELS.EVENT_TYPE]
  );

  eventsProcessed.inc({
    [ES_METRIC_LABELS.PROJECTION_NAME]: projectionName,
    [ES_METRIC_LABELS.EVENT_TYPE]: eventType,
  });
}

/**
 * Update projection lag (events behind head).
 */
export function setProjectionLag(projectionName: string, lag: number): void {
  const lagGauge = gauge(
    ES_METRICS.PROJECTION_LAG,
    'Number of events projection is behind head',
    [ES_METRIC_LABELS.PROJECTION_NAME]
  );

  lagGauge.set(lag, {
    [ES_METRIC_LABELS.PROJECTION_NAME]: projectionName,
  });
}

/**
 * Record a projection error.
 */
export function recordProjectionError(
  projectionName: string,
  errorType: string
): void {
  const errors = counter(
    ES_METRICS.PROJECTION_ERRORS,
    'Total projection errors',
    [ES_METRIC_LABELS.PROJECTION_NAME, ES_METRIC_LABELS.ERROR_TYPE]
  );

  errors.inc({
    [ES_METRIC_LABELS.PROJECTION_NAME]: projectionName,
    [ES_METRIC_LABELS.ERROR_TYPE]: errorType,
  });
}

/**
 * Create a timer for measuring projection processing duration.
 */
export function startProjectionProcessTimer(projectionName: string): () => number {
  const hist = histogram(
    ES_METRICS.PROJECTION_PROCESS_DURATION,
    'Time to process an event in a projection',
    DEFAULT_LATENCY_BUCKETS,
    [ES_METRIC_LABELS.PROJECTION_NAME]
  );

  return hist.startTimer({
    [ES_METRIC_LABELS.PROJECTION_NAME]: projectionName,
  });
}

// ============================================================================
// FAILURE HANDLING METRICS
// ============================================================================

/**
 * Update dead letter queue size.
 */
export function setDlqSize(projectionName: string, size: number): void {
  const dlqGauge = gauge(
    ES_METRICS.DLQ_SIZE,
    'Number of events in dead letter queue',
    [ES_METRIC_LABELS.PROJECTION_NAME]
  );

  dlqGauge.set(size, {
    [ES_METRIC_LABELS.PROJECTION_NAME]: projectionName,
  });
}

/**
 * Record a retry attempt.
 */
export function recordRetry(projectionName: string, attempt: number): void {
  const retries = counter(
    ES_METRICS.RETRIES_TOTAL,
    'Total retry attempts',
    [ES_METRIC_LABELS.PROJECTION_NAME]
  );

  retries.inc({
    [ES_METRIC_LABELS.PROJECTION_NAME]: projectionName,
  });
}

// ============================================================================
// INSTRUMENTATION HELPERS
// ============================================================================

/**
 * Helper class for instrumenting operations.
 *
 * @example
 * ```typescript
 * const instrumentation = new ESInstrumentation();
 *
 * // In repository save:
 * await instrumentation.instrumentSave(aggregate, async () => {
 *   await eventStore.append(stream, events);
 * });
 *
 * // In projection:
 * await instrumentation.instrumentProjection('OrderSummary', event, async () => {
 *   await handleEvent(event);
 * });
 * ```
 */
export class ESInstrumentation {
  /**
   * Instrument an aggregate save operation.
   */
  async instrumentSave<T>(
    aggregateType: string,
    eventTypes: string[],
    tenantId: string | undefined,
    operation: () => Promise<T>
  ): Promise<T> {
    const endTimer = startAppendTimer(aggregateType);

    try {
      const result = await operation();
      recordEventsAppended(aggregateType, eventTypes, tenantId);
      return result;
    } finally {
      endTimer();
    }
  }

  /**
   * Instrument an aggregate load operation.
   */
  async instrumentLoad<T>(
    aggregateType: string,
    operation: () => Promise<T>
  ): Promise<T> {
    const endTimer = startAggregateLoadTimer(aggregateType);

    try {
      return await operation();
    } finally {
      endTimer();
    }
  }

  /**
   * Instrument a command execution.
   */
  async instrumentCommand<T>(
    commandType: string,
    aggregateType: string,
    operation: () => Promise<T>
  ): Promise<T> {
    const endTimer = startCommandTimer(commandType, aggregateType);

    try {
      return await operation();
    } finally {
      endTimer();
    }
  }

  /**
   * Instrument projection event processing.
   */
  async instrumentProjection<T>(
    projectionName: string,
    eventType: string,
    operation: () => Promise<T>
  ): Promise<T> {
    const endTimer = startProjectionProcessTimer(projectionName);

    try {
      const result = await operation();
      recordProjectionEventProcessed(projectionName, eventType);
      return result;
    } catch (error) {
      recordProjectionError(
        projectionName,
        error instanceof Error ? error.constructor.name : 'UnknownError'
      );
      throw error;
    } finally {
      endTimer();
    }
  }
}

/**
 * Shared instrumentation instance.
 */
export const esInstrumentation = new ESInstrumentation();
