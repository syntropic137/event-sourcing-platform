/**
 * Observability Types
 *
 * Core type definitions for the observability module.
 */

// ============================================================================
// TRACING TYPES
// ============================================================================

/**
 * Span status values following OpenTelemetry conventions.
 */
export type SpanStatus = 'unset' | 'ok' | 'error';

/**
 * Span kind following OpenTelemetry conventions.
 */
export type SpanKind = 'internal' | 'server' | 'client' | 'producer' | 'consumer';

/**
 * Span attributes (key-value pairs).
 */
export type SpanAttributes = Record<string, string | number | boolean | undefined>;

/**
 * A span represents a unit of work.
 */
export interface Span {
  /** Span name/operation */
  readonly name: string;

  /** Set span status */
  setStatus(status: SpanStatus, message?: string): void;

  /** Add attributes to the span */
  setAttribute(key: string, value: string | number | boolean): void;

  /** Add multiple attributes */
  setAttributes(attributes: SpanAttributes): void;

  /** Record an exception */
  recordException(error: Error): void;

  /** Add an event to the span */
  addEvent(name: string, attributes?: SpanAttributes): void;

  /** End the span */
  end(): void;
}

/**
 * Tracer interface for creating spans.
 */
export interface Tracer {
  /** Start a new span */
  startSpan(name: string, options?: StartSpanOptions): Span;

  /** Execute function within a span */
  withSpan<T>(name: string, fn: (span: Span) => T, options?: StartSpanOptions): T;

  /** Execute async function within a span */
  withSpanAsync<T>(
    name: string,
    fn: (span: Span) => Promise<T>,
    options?: StartSpanOptions
  ): Promise<T>;
}

/**
 * Options for starting a span.
 */
export interface StartSpanOptions {
  /** Span kind */
  kind?: SpanKind;

  /** Initial attributes */
  attributes?: SpanAttributes;

  /** Parent context (for distributed tracing) */
  parentContext?: TracingContextData;
}

/**
 * Data carried by TracingContext.
 */
export interface TracingContextData {
  /** Correlation ID linking related operations */
  correlationId?: string;

  /** Causation ID linking cause and effect */
  causationId?: string;

  /** Actor who initiated the operation */
  actorId?: string;

  /** Tenant for multi-tenancy */
  tenantId?: string;

  /** W3C traceparent header value */
  traceparent?: string;

  /** W3C tracestate header value */
  tracestate?: string;

  /** Additional baggage items */
  baggage?: Record<string, string>;
}

// ============================================================================
// METRICS TYPES
// ============================================================================

/**
 * Metric labels (key-value pairs for dimensional metrics).
 */
export type MetricLabels = Record<string, string>;

/**
 * Counter metric - monotonically increasing value.
 */
export interface Counter {
  /** Increment by 1 */
  inc(labels?: MetricLabels): void;

  /** Increment by value */
  add(value: number, labels?: MetricLabels): void;
}

/**
 * Gauge metric - value that can go up and down.
 */
export interface Gauge {
  /** Set value */
  set(value: number, labels?: MetricLabels): void;

  /** Increment by 1 */
  inc(labels?: MetricLabels): void;

  /** Decrement by 1 */
  dec(labels?: MetricLabels): void;

  /** Add to current value */
  add(value: number, labels?: MetricLabels): void;
}

/**
 * Histogram metric - distribution of values.
 */
export interface Histogram {
  /** Observe a value */
  observe(value: number, labels?: MetricLabels): void;

  /** Start a timer, returns function to end timer */
  startTimer(labels?: MetricLabels): () => number;
}

/**
 * Timer that records duration.
 */
export interface Timer {
  /** End the timer and return duration in seconds */
  end(): number;
}

/**
 * Metrics registry for creating and managing metrics.
 */
export interface MetricsRegistry {
  /** Create or get a counter */
  counter(name: string, help: string, labelNames?: string[]): Counter;

  /** Create or get a gauge */
  gauge(name: string, help: string, labelNames?: string[]): Gauge;

  /** Create or get a histogram */
  histogram(name: string, help: string, buckets?: number[], labelNames?: string[]): Histogram;

  /** Get metrics in Prometheus format */
  getMetrics(): Promise<string>;

  /** Reset all metrics */
  resetMetrics(): void;
}

// ============================================================================
// LOGGING TYPES
// ============================================================================

/**
 * Log levels following standard conventions.
 */
export type LogLevel = 'trace' | 'debug' | 'info' | 'warn' | 'error' | 'fatal';

/**
 * Log context - additional data to include in log entries.
 */
export type LogContext = Record<string, unknown>;

/**
 * Structured log entry.
 */
export interface LogEntry {
  /** ISO timestamp */
  timestamp: string;

  /** Log level */
  level: LogLevel;

  /** Log message */
  message: string;

  /** Component/logger name */
  component?: string;

  /** Correlation ID */
  correlationId?: string;

  /** Causation ID */
  causationId?: string;

  /** Actor ID */
  actorId?: string;

  /** Additional context */
  [key: string]: unknown;
}

/**
 * Logger interface for structured logging.
 */
export interface Logger {
  /** Log at trace level */
  trace(message: string, context?: LogContext): void;

  /** Log at debug level */
  debug(message: string, context?: LogContext): void;

  /** Log at info level */
  info(message: string, context?: LogContext): void;

  /** Log at warn level */
  warn(message: string, context?: LogContext): void;

  /** Log at error level */
  error(message: string, context?: LogContext): void;

  /** Log at fatal level */
  fatal(message: string, context?: LogContext): void;

  /** Create a child logger with additional context */
  child(context: LogContext): Logger;

  /** Create a child logger for a component */
  forComponent(component: string): Logger;
}

/**
 * Log output destination.
 */
export interface LogOutput {
  /** Write a log entry */
  write(entry: LogEntry): void;
}

// ============================================================================
// STANDARD METRIC NAMES (ES-specific)
// ============================================================================

/**
 * Standard metric names for Event Sourcing operations.
 * Use these for consistency across implementations.
 */
export const ES_METRICS = {
  // Event Store operations
  EVENTS_APPENDED_TOTAL: 'es_events_appended_total',
  EVENTS_APPEND_DURATION: 'es_events_append_duration_seconds',
  AGGREGATE_LOAD_DURATION: 'es_aggregate_load_duration_seconds',
  COMMAND_DURATION: 'es_command_duration_seconds',

  // Projection operations
  PROJECTION_EVENTS_PROCESSED: 'es_projection_events_processed_total',
  PROJECTION_LAG: 'es_projection_lag_events',
  PROJECTION_ERRORS: 'es_projection_errors_total',
  PROJECTION_PROCESS_DURATION: 'es_projection_process_duration_seconds',

  // Failure handling
  DLQ_SIZE: 'es_dlq_size',
  RETRIES_TOTAL: 'es_retries_total',
} as const;

/**
 * Standard label names for ES metrics.
 */
export const ES_METRIC_LABELS = {
  AGGREGATE_TYPE: 'aggregate_type',
  EVENT_TYPE: 'event_type',
  COMMAND_TYPE: 'command_type',
  PROJECTION_NAME: 'projection_name',
  TENANT_ID: 'tenant_id',
  ERROR_TYPE: 'error_type',
} as const;

// ============================================================================
// HEADER CONVENTIONS
// ============================================================================

/**
 * Standard header keys for observability data propagation.
 */
export const HEADER_KEYS = {
  /** W3C Trace Context - trace parent */
  TRACE_PARENT: 'traceparent',

  /** W3C Trace Context - trace state */
  TRACE_STATE: 'tracestate',

  /** W3C Baggage */
  BAGGAGE: 'baggage',

  /** Correlation ID header */
  CORRELATION_ID: 'x-correlation-id',

  /** Causation ID header */
  CAUSATION_ID: 'x-causation-id',

  /** Request ID header */
  REQUEST_ID: 'x-request-id',

  /** Actor/User ID header */
  ACTOR_ID: 'x-actor-id',

  /** Tenant ID header */
  TENANT_ID: 'x-tenant-id',
} as const;
