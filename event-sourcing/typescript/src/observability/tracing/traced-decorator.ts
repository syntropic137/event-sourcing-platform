/**
 * @Traced Decorator
 *
 * Decorator for automatic span creation around methods.
 */

import {
  Span,
  SpanAttributes,
  SpanKind,
  SpanStatus,
  Tracer,
} from '../types';
import { getCurrentContext } from './tracing-context';

// ============================================================================
// NO-OP IMPLEMENTATIONS
// ============================================================================

/**
 * No-op span that does nothing.
 * Used when no tracer is configured.
 */
class NoOpSpan implements Span {
  readonly name: string;

  constructor(name: string) {
    this.name = name;
  }

  setStatus(_status: SpanStatus, _message?: string): void {
    // No-op
  }

  setAttribute(_key: string, _value: string | number | boolean): void {
    // No-op
  }

  setAttributes(_attributes: SpanAttributes): void {
    // No-op
  }

  recordException(_error: Error): void {
    // No-op
  }

  addEvent(_name: string, _attributes?: SpanAttributes): void {
    // No-op
  }

  end(): void {
    // No-op
  }
}

/**
 * No-op tracer that creates no-op spans.
 * Used when OpenTelemetry is not configured.
 */
class NoOpTracer implements Tracer {
  startSpan(name: string): Span {
    return new NoOpSpan(name);
  }

  withSpan<T>(name: string, fn: (span: Span) => T): T {
    return fn(new NoOpSpan(name));
  }

  async withSpanAsync<T>(name: string, fn: (span: Span) => Promise<T>): Promise<T> {
    return fn(new NoOpSpan(name));
  }
}

// ============================================================================
// GLOBAL TRACER REGISTRY
// ============================================================================

let globalTracer: Tracer = new NoOpTracer();

/**
 * Set the global tracer.
 *
 * Call this once at application startup with your configured tracer.
 *
 * @example
 * ```typescript
 * import { trace } from '@opentelemetry/api';
 * import { setGlobalTracer, createOTelTracer } from '@event-sourcing-platform/typescript/observability';
 *
 * const tracer = createOTelTracer(trace.getTracer('my-service'));
 * setGlobalTracer(tracer);
 * ```
 */
export function setGlobalTracer(tracer: Tracer): void {
  globalTracer = tracer;
}

/**
 * Get the global tracer.
 */
export function getGlobalTracer(): Tracer {
  return globalTracer;
}

/**
 * Reset to no-op tracer.
 * Useful for testing.
 */
export function resetGlobalTracer(): void {
  globalTracer = new NoOpTracer();
}

// ============================================================================
// @TRACED DECORATOR
// ============================================================================

/**
 * Options for the @Traced decorator.
 */
export interface TracedOptions {
  /** Span name (defaults to "ClassName.methodName") */
  name?: string;

  /** Span kind */
  kind?: SpanKind;

  /** Static attributes to add to every span */
  attributes?: SpanAttributes;

  /** Whether to record exceptions (default: true) */
  recordExceptions?: boolean;

  /** Extract attributes from method arguments */
  extractAttributes?: (args: unknown[]) => SpanAttributes;
}

/**
 * @Traced - Decorator for automatic span creation.
 *
 * Wraps a method to automatically create a span for each invocation.
 * The span includes:
 * - Correlation/causation IDs from current context
 * - Execution status (ok/error)
 * - Exception recording on failure
 *
 * @example
 * ```typescript
 * class OrderProjection {
 *   @Traced('OrderProjection.handleOrderPlaced')
 *   async handleOrderPlaced(event: OrderPlacedEvent): Promise<void> {
 *     // Span automatically created and ended
 *   }
 * }
 * ```
 *
 * @example
 * ```typescript
 * class OrderService {
 *   @Traced({
 *     name: 'OrderService.createOrder',
 *     kind: 'server',
 *     extractAttributes: (args) => ({
 *       'order.customer_id': (args[0] as CreateOrderCommand).customerId,
 *     }),
 *   })
 *   async createOrder(command: CreateOrderCommand): Promise<Order> {
 *     // ...
 *   }
 * }
 * ```
 */
export function Traced(
  nameOrOptions?: string | TracedOptions
): MethodDecorator {
  return function (
    target: object,
    propertyKey: string | symbol,
    descriptor: PropertyDescriptor
  ): PropertyDescriptor {
    const originalMethod = descriptor.value;

    if (typeof originalMethod !== 'function') {
      throw new Error('@Traced can only be applied to methods');
    }

    // Parse options
    const options: TracedOptions =
      typeof nameOrOptions === 'string'
        ? { name: nameOrOptions }
        : nameOrOptions ?? {};

    const recordExceptions = options.recordExceptions ?? true;
    const spanKind = options.kind ?? 'internal';

    // Build span name
    const className = target.constructor.name;
    const methodName = String(propertyKey);
    const spanName = options.name ?? `${className}.${methodName}`;

    descriptor.value = function (...args: unknown[]): unknown {
      const tracer = getGlobalTracer();

      // Get context-based attributes
      const ctx = getCurrentContext();
      const contextAttrs = ctx.toSpanAttributes();

      // Get static attributes
      const staticAttrs = options.attributes ?? {};

      // Get dynamic attributes from args
      const dynamicAttrs = options.extractAttributes
        ? options.extractAttributes(args)
        : {};

      // Combine all attributes
      const attributes: SpanAttributes = {
        ...contextAttrs,
        ...staticAttrs,
        ...dynamicAttrs,
      };

      // Start span
      const span = tracer.startSpan(spanName, {
        kind: spanKind,
        attributes,
        parentContext: ctx.getData(),
      });

      // Check if result is a promise
      const result = originalMethod.apply(this, args);

      if (result instanceof Promise) {
        return result
          .then((value) => {
            span.setStatus('ok');
            span.end();
            return value;
          })
          .catch((error) => {
            span.setStatus('error', error.message);
            if (recordExceptions && error instanceof Error) {
              span.recordException(error);
            }
            span.end();
            throw error;
          });
      }

      // Synchronous method
      span.setStatus('ok');
      span.end();
      return result;
    };

    return descriptor;
  };
}

// ============================================================================
// MANUAL SPAN UTILITIES
// ============================================================================

/**
 * Start a new span manually.
 *
 * @example
 * ```typescript
 * const span = startSpan('myOperation', {
 *   kind: 'internal',
 *   attributes: { 'my.attr': 'value' },
 * });
 *
 * try {
 *   // ... do work
 *   span.setStatus('ok');
 * } catch (error) {
 *   span.setStatus('error', error.message);
 *   span.recordException(error);
 *   throw error;
 * } finally {
 *   span.end();
 * }
 * ```
 */
export function startSpan(
  name: string,
  options?: {
    kind?: SpanKind;
    attributes?: SpanAttributes;
  }
): Span {
  const tracer = getGlobalTracer();
  const ctx = getCurrentContext();

  return tracer.startSpan(name, {
    kind: options?.kind,
    attributes: {
      ...ctx.toSpanAttributes(),
      ...options?.attributes,
    },
    parentContext: ctx.getData(),
  });
}

/**
 * Execute a function within a span.
 *
 * @example
 * ```typescript
 * const result = withSpan('processOrder', (span) => {
 *   span.setAttribute('order.id', orderId);
 *   return processOrder(orderId);
 * });
 * ```
 */
export function withSpan<T>(
  name: string,
  fn: (span: Span) => T,
  options?: {
    kind?: SpanKind;
    attributes?: SpanAttributes;
  }
): T {
  const span = startSpan(name, options);

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

/**
 * Execute an async function within a span.
 *
 * @example
 * ```typescript
 * const result = await withSpanAsync('fetchOrder', async (span) => {
 *   span.setAttribute('order.id', orderId);
 *   return await fetchOrder(orderId);
 * });
 * ```
 */
export async function withSpanAsync<T>(
  name: string,
  fn: (span: Span) => Promise<T>,
  options?: {
    kind?: SpanKind;
    attributes?: SpanAttributes;
  }
): Promise<T> {
  const span = startSpan(name, options);

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

// Re-export NoOpSpan and NoOpTracer for testing
export { NoOpSpan, NoOpTracer };
