/**
 * OpenTelemetry Integration
 *
 * Adapter for using OpenTelemetry as the tracing backend.
 */

import { Span, Tracer, SpanAttributes, SpanKind, SpanStatus, StartSpanOptions } from '../types';

// ============================================================================
// TYPE DEFINITIONS FOR OTEL API
// ============================================================================

/**
 * OpenTelemetry Span interface (from @opentelemetry/api).
 * We define our own interface to avoid requiring the package at runtime.
 */
interface OTelSpan {
  setAttribute(key: string, value: unknown): this;
  setStatus(status: { code: number; message?: string }): this;
  recordException(exception: Error): void;
  addEvent(name: string, attributes?: Record<string, unknown>): this;
  end(): void;
}

/**
 * OpenTelemetry Tracer interface (from @opentelemetry/api).
 */
interface OTelTracer {
  startSpan(
    name: string,
    options?: {
      kind?: number;
      attributes?: Record<string, unknown>;
    }
  ): OTelSpan;
}

/**
 * OTel SpanStatusCode values.
 */
const OTelSpanStatusCode = {
  UNSET: 0,
  OK: 1,
  ERROR: 2,
} as const;

/**
 * OTel SpanKind values.
 */
const OTelSpanKind = {
  INTERNAL: 0,
  SERVER: 1,
  CLIENT: 2,
  PRODUCER: 3,
  CONSUMER: 4,
} as const;

// ============================================================================
// OTEL SPAN ADAPTER
// ============================================================================

/**
 * Adapter that wraps an OpenTelemetry span.
 */
class OTelSpanAdapter implements Span {
  readonly name: string;
  private readonly otelSpan: OTelSpan;

  constructor(name: string, otelSpan: OTelSpan) {
    this.name = name;
    this.otelSpan = otelSpan;
  }

  setStatus(status: SpanStatus, message?: string): void {
    const code =
      status === 'ok'
        ? OTelSpanStatusCode.OK
        : status === 'error'
          ? OTelSpanStatusCode.ERROR
          : OTelSpanStatusCode.UNSET;

    this.otelSpan.setStatus({ code, message });
  }

  setAttribute(key: string, value: string | number | boolean): void {
    this.otelSpan.setAttribute(key, value);
  }

  setAttributes(attributes: SpanAttributes): void {
    for (const [key, value] of Object.entries(attributes)) {
      if (value !== undefined) {
        this.otelSpan.setAttribute(key, value);
      }
    }
  }

  recordException(error: Error): void {
    this.otelSpan.recordException(error);
  }

  addEvent(name: string, attributes?: SpanAttributes): void {
    this.otelSpan.addEvent(name, attributes as Record<string, unknown> | undefined);
  }

  end(): void {
    this.otelSpan.end();
  }
}

// ============================================================================
// OTEL TRACER ADAPTER
// ============================================================================

/**
 * Configuration for the OpenTelemetry tracer adapter.
 */
export interface OTelTracerConfig {
  /** The OpenTelemetry tracer instance */
  tracer: OTelTracer;

  /** Service name for span attributes */
  serviceName?: string;

  /** Default attributes to add to all spans */
  defaultAttributes?: SpanAttributes;
}

/**
 * Adapter that wraps an OpenTelemetry tracer.
 */
class OTelTracerAdapter implements Tracer {
  private readonly otelTracer: OTelTracer;
  private readonly serviceName?: string;
  private readonly defaultAttributes: SpanAttributes;

  constructor(config: OTelTracerConfig) {
    this.otelTracer = config.tracer;
    this.serviceName = config.serviceName;
    this.defaultAttributes = config.defaultAttributes ?? {};
  }

  private mapSpanKind(kind?: SpanKind): number {
    switch (kind) {
      case 'server':
        return OTelSpanKind.SERVER;
      case 'client':
        return OTelSpanKind.CLIENT;
      case 'producer':
        return OTelSpanKind.PRODUCER;
      case 'consumer':
        return OTelSpanKind.CONSUMER;
      case 'internal':
      default:
        return OTelSpanKind.INTERNAL;
    }
  }

  startSpan(name: string, options?: StartSpanOptions): Span {
    const allAttributes: Record<string, unknown> = {
      ...this.defaultAttributes,
    };

    if (this.serviceName) {
      allAttributes['service.name'] = this.serviceName;
    }

    // Add parent context attributes
    if (options?.parentContext) {
      if (options.parentContext.correlationId) {
        allAttributes['es.correlation_id'] = options.parentContext.correlationId;
      }
      if (options.parentContext.causationId) {
        allAttributes['es.causation_id'] = options.parentContext.causationId;
      }
      if (options.parentContext.actorId) {
        allAttributes['es.actor_id'] = options.parentContext.actorId;
      }
      if (options.parentContext.tenantId) {
        allAttributes['es.tenant_id'] = options.parentContext.tenantId;
      }
    }

    // Add provided attributes
    if (options?.attributes) {
      for (const [key, value] of Object.entries(options.attributes)) {
        if (value !== undefined) {
          allAttributes[key] = value;
        }
      }
    }

    const otelSpan = this.otelTracer.startSpan(name, {
      kind: this.mapSpanKind(options?.kind),
      attributes: allAttributes,
    });

    return new OTelSpanAdapter(name, otelSpan);
  }

  withSpan<T>(name: string, fn: (span: Span) => T, options?: StartSpanOptions): T {
    const span = this.startSpan(name, options);

    try {
      const result = fn(span);
      span.setStatus('ok');
      return result;
    } catch (error) {
      span.setStatus('error', error instanceof Error ? error.message : String(error));
      if (error instanceof Error) {
        span.recordException(error);
      }
      throw error;
    } finally {
      span.end();
    }
  }

  async withSpanAsync<T>(
    name: string,
    fn: (span: Span) => Promise<T>,
    options?: StartSpanOptions
  ): Promise<T> {
    const span = this.startSpan(name, options);

    try {
      const result = await fn(span);
      span.setStatus('ok');
      return result;
    } catch (error) {
      span.setStatus('error', error instanceof Error ? error.message : String(error));
      if (error instanceof Error) {
        span.recordException(error);
      }
      throw error;
    } finally {
      span.end();
    }
  }
}

// ============================================================================
// FACTORY FUNCTIONS
// ============================================================================

/**
 * Create a tracer adapter from an OpenTelemetry tracer.
 *
 * @example
 * ```typescript
 * import { trace } from '@opentelemetry/api';
 * import { createOTelTracer, setGlobalTracer } from '@syntropic137/event-sourcing-typescript/observability';
 *
 * const otelTracer = trace.getTracer('my-service', '1.0.0');
 * const tracer = createOTelTracer({
 *   tracer: otelTracer,
 *   serviceName: 'order-service',
 * });
 *
 * setGlobalTracer(tracer);
 * ```
 */
export function createOTelTracer(config: OTelTracerConfig): Tracer {
  return new OTelTracerAdapter(config);
}

/**
 * Simple helper to create tracer from just the OTel tracer instance.
 *
 * @example
 * ```typescript
 * import { trace } from '@opentelemetry/api';
 * import { fromOTelTracer } from '@syntropic137/event-sourcing-typescript/observability';
 *
 * const tracer = fromOTelTracer(trace.getTracer('my-service'));
 * ```
 */
export function fromOTelTracer(otelTracer: OTelTracer, serviceName?: string): Tracer {
  return new OTelTracerAdapter({
    tracer: otelTracer,
    serviceName,
  });
}

// ============================================================================
// W3C TRACE CONTEXT UTILITIES
// ============================================================================

/**
 * Parse a W3C traceparent header.
 *
 * Format: {version}-{trace-id}-{parent-id}-{trace-flags}
 * Example: 00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01
 */
export function parseTraceparent(
  traceparent: string
): { traceId: string; parentId: string; traceFlags: number } | null {
  const parts = traceparent.split('-');

  if (parts.length !== 4) {
    return null;
  }

  const [version, traceId, parentId, flags] = parts;

  // Currently only version 00 is supported
  if (version !== '00') {
    return null;
  }

  // Validate format
  if (traceId.length !== 32 || parentId.length !== 16 || flags.length !== 2) {
    return null;
  }

  return {
    traceId,
    parentId,
    traceFlags: parseInt(flags, 16),
  };
}

/**
 * Generate a new traceparent header.
 */
export function generateTraceparent(
  traceId?: string,
  parentId?: string,
  sampled: boolean = true
): string {
  const version = '00';
  const newTraceId = traceId ?? generateTraceId();
  const newParentId = parentId ?? generateSpanId();
  const flags = sampled ? '01' : '00';

  return `${version}-${newTraceId}-${newParentId}-${flags}`;
}

/**
 * Generate a random trace ID (32 hex characters).
 */
export function generateTraceId(): string {
  return generateRandomHex(32);
}

/**
 * Generate a random span ID (16 hex characters).
 */
export function generateSpanId(): string {
  return generateRandomHex(16);
}

/**
 * Generate random hex string of specified length.
 */
function generateRandomHex(length: number): string {
  const chars = '0123456789abcdef';
  let result = '';
  for (let i = 0; i < length; i++) {
    result += chars[Math.floor(Math.random() * 16)];
  }
  return result;
}
