/**
 * Observability Module
 *
 * Provides tracing, metrics, and logging utilities for Event Sourcing applications.
 *
 * @example
 * ```typescript
 * import {
 *   // Tracing
 *   TracingContext,
 *   Traced,
 *   runWithContext,
 *
 *   // Metrics
 *   counter,
 *   gauge,
 *   histogram,
 *   esInstrumentation,
 *
 *   // Logging
 *   StructuredLogger,
 *   info,
 *   error,
 * } from '@event-sourcing-platform/typescript/observability';
 *
 * // Extract context from request
 * const ctx = TracingContext.fromRequest(req);
 *
 * // Run operation with context
 * await runWithContext(ctx, async () => {
 *   // Context flows automatically
 *   info('Processing order', { orderId });
 *
 *   // Metrics
 *   await esInstrumentation.instrumentCommand('PlaceOrder', 'Order', async () => {
 *     // ... handle command
 *   });
 * });
 * ```
 */

// ============================================================================
// TYPES
// ============================================================================

export {
  // Span types
  Span,
  SpanStatus,
  SpanKind,
  SpanAttributes,
  Tracer,
  StartSpanOptions,
  TracingContextData,

  // Metric types
  Counter,
  Gauge,
  Histogram,
  Timer,
  MetricLabels,
  MetricsRegistry,

  // Logging types
  LogLevel,
  LogContext,
  LogEntry,
  Logger,
  LogOutput,

  // Constants
  ES_METRICS,
  ES_METRIC_LABELS,
  HEADER_KEYS,
} from './types';

// ============================================================================
// TRACING
// ============================================================================

export {
  // Core context
  TracingContext,
  runWithContext,
  getCurrentContext,
  hasContext,
  contextStorage,

  // Types
  RequestHeaders,
  IncomingRequest,
  CommandMetadata,
  EventWithMetadata,
} from './tracing/tracing-context';

export {
  // Decorator
  Traced,
  TracedOptions,

  // Global tracer
  setGlobalTracer,
  getGlobalTracer,
  resetGlobalTracer,

  // Manual span utilities
  startSpan,
  withSpan,
  withSpanAsync,

  // No-op implementations for testing
  NoOpSpan,
  NoOpTracer,
} from './tracing/traced-decorator';

export {
  // OpenTelemetry integration
  createOTelTracer,
  fromOTelTracer,
  OTelTracerConfig,

  // W3C Trace Context utilities
  parseTraceparent,
  generateTraceparent,
  generateTraceId,
  generateSpanId,
} from './tracing/opentelemetry';

// ============================================================================
// METRICS
// ============================================================================

export {
  // Registry
  InMemoryMetricsRegistry,
  getGlobalRegistry,
  setGlobalRegistry,
  resetGlobalRegistry,

  // Convenience functions
  counter,
  gauge,
  histogram,
  getMetrics,
  resetMetrics,

  // Timer
  startTimer,
  startHistogramTimer,

  // Constants
  DEFAULT_LATENCY_BUCKETS,
} from './metrics/metrics';

export {
  // ES-specific metrics
  recordEventsAppended,
  startAppendTimer,
  startAggregateLoadTimer,
  startCommandTimer,
  recordProjectionEventProcessed,
  setProjectionLag,
  recordProjectionError,
  startProjectionProcessTimer,
  setDlqSize,
  recordRetry,

  // Instrumentation helper
  ESInstrumentation,
  esInstrumentation,
} from './metrics/es-metrics';

// ============================================================================
// LOGGING
// ============================================================================

export {
  // Logger implementation
  StructuredLogger,
  StructuredLoggerOptions,

  // Outputs
  ConsoleJsonOutput,
  ConsolePrettyOutput,
  NoOpOutput,
  CollectorOutput,

  // Global logger
  getGlobalLogger,
  setGlobalLogger,
  configureGlobalLogger,
  resetGlobalLogger,

  // Convenience functions
  trace,
  debug,
  info,
  warn,
  error,
  fatal,
  forComponent,

  // ES-specific helpers
  ESLogMessages,
} from './logging/logger';
