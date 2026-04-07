/**
 * Tracing Context
 *
 * Utility for propagating correlation and causation IDs through the
 * command → event → projection flow.
 */

import { v4 as uuidv4 } from 'uuid';
import { TracingContextData, HEADER_KEYS, SpanAttributes } from '../types';

/**
 * Headers from an HTTP-like request.
 */
export interface RequestHeaders {
  get(name: string): string | null | undefined;
}

/**
 * Simplified request interface for extracting context.
 */
export interface IncomingRequest {
  headers: RequestHeaders | Record<string, string | string[] | undefined>;
}

/**
 * Command metadata that can carry tracing context.
 */
export interface CommandMetadata {
  correlationId?: string;
  causationId?: string;
  actorId?: string;
  tenantId?: string;
}

/**
 * Event envelope with metadata.
 */
export interface EventWithMetadata {
  metadata?: {
    correlationId?: string;
    causationId?: string;
    eventId?: string;
    actorId?: string;
    tenantId?: string;
  };
}

/**
 * TracingContext - Immutable context for request tracing.
 *
 * This class carries correlation and causation IDs through the system,
 * enabling end-to-end tracing of requests.
 *
 * @example
 * ```typescript
 * // From HTTP request
 * const ctx = TracingContext.fromRequest(req);
 *
 * // To command metadata
 * const command = new PlaceOrderCommand(...);
 * command.metadata = ctx.toCommandMetadata();
 *
 * // From event (in projection)
 * const eventCtx = TracingContext.fromEvent(event);
 * ```
 */
export class TracingContext {
  private readonly data: Readonly<TracingContextData>;

  private constructor(data: TracingContextData) {
    this.data = Object.freeze({ ...data });
  }

  // ============================================================================
  // FACTORY METHODS
  // ============================================================================

  /**
   * Create a new root context with a fresh correlation ID.
   */
  static create(options: Partial<TracingContextData> = {}): TracingContext {
    return new TracingContext({
      correlationId: options.correlationId ?? uuidv4(),
      causationId: options.causationId,
      actorId: options.actorId,
      tenantId: options.tenantId,
      traceparent: options.traceparent,
      tracestate: options.tracestate,
      baggage: options.baggage,
    });
  }

  /**
   * Create an empty context (no tracing data).
   */
  static empty(): TracingContext {
    return new TracingContext({});
  }

  /**
   * Extract tracing context from an HTTP-like request.
   *
   * Looks for standard headers:
   * - X-Correlation-ID
   * - X-Causation-ID
   * - X-Actor-ID
   * - X-Tenant-ID
   * - traceparent (W3C)
   * - tracestate (W3C)
   */
  static fromRequest(request: IncomingRequest): TracingContext {
    const getHeader = (name: string): string | undefined => {
      const headers = request.headers;

      // Handle Map-like headers (Express, Fastify)
      if (typeof (headers as RequestHeaders).get === 'function') {
        const value = (headers as RequestHeaders).get(name);
        return value ?? undefined;
      }

      // Handle plain object headers
      const headerObj = headers as Record<string, string | string[] | undefined>;
      const value = headerObj[name] ?? headerObj[name.toLowerCase()];

      if (Array.isArray(value)) {
        return value[0];
      }
      return value;
    };

    return new TracingContext({
      correlationId: getHeader(HEADER_KEYS.CORRELATION_ID) ?? uuidv4(),
      causationId: getHeader(HEADER_KEYS.CAUSATION_ID),
      actorId: getHeader(HEADER_KEYS.ACTOR_ID),
      tenantId: getHeader(HEADER_KEYS.TENANT_ID),
      traceparent: getHeader(HEADER_KEYS.TRACE_PARENT),
      tracestate: getHeader(HEADER_KEYS.TRACE_STATE),
    });
  }

  /**
   * Create context from command metadata.
   */
  static fromCommand(command: { metadata?: CommandMetadata }): TracingContext {
    const metadata = command.metadata ?? {};
    return new TracingContext({
      correlationId: metadata.correlationId,
      causationId: metadata.causationId,
      actorId: metadata.actorId,
      tenantId: metadata.tenantId,
    });
  }

  /**
   * Create context from an event envelope.
   *
   * The event's eventId becomes the causationId for downstream operations.
   */
  static fromEvent(event: EventWithMetadata): TracingContext {
    const metadata = event.metadata ?? {};
    return new TracingContext({
      correlationId: metadata.correlationId,
      // Event ID becomes causation for downstream operations
      causationId: metadata.eventId ?? metadata.causationId,
      actorId: metadata.actorId,
      tenantId: metadata.tenantId,
    });
  }

  /**
   * Create context from raw data.
   */
  static fromData(data: TracingContextData): TracingContext {
    return new TracingContext(data);
  }

  // ============================================================================
  // ACCESSORS
  // ============================================================================

  /** Get correlation ID */
  get correlationId(): string | undefined {
    return this.data.correlationId;
  }

  /** Get causation ID */
  get causationId(): string | undefined {
    return this.data.causationId;
  }

  /** Get actor ID */
  get actorId(): string | undefined {
    return this.data.actorId;
  }

  /** Get tenant ID */
  get tenantId(): string | undefined {
    return this.data.tenantId;
  }

  /** Get W3C traceparent */
  get traceparent(): string | undefined {
    return this.data.traceparent;
  }

  /** Get W3C tracestate */
  get tracestate(): string | undefined {
    return this.data.tracestate;
  }

  /** Get baggage items */
  get baggage(): Record<string, string> | undefined {
    return this.data.baggage;
  }

  /** Get all context data */
  getData(): TracingContextData {
    return { ...this.data };
  }

  /** Check if context has any tracing data */
  isEmpty(): boolean {
    return !this.data.correlationId && !this.data.traceparent;
  }

  // ============================================================================
  // TRANSFORMATIONS
  // ============================================================================

  /**
   * Create a child context with updated causation.
   *
   * Use this when an operation produces a new "cause" (e.g., a command
   * produces events, an event triggers a saga step).
   */
  withCausation(causationId: string): TracingContext {
    return new TracingContext({
      ...this.data,
      causationId,
    });
  }

  /**
   * Create a child context with actor information.
   */
  withActor(actorId: string): TracingContext {
    return new TracingContext({
      ...this.data,
      actorId,
    });
  }

  /**
   * Create a child context with tenant information.
   */
  withTenant(tenantId: string): TracingContext {
    return new TracingContext({
      ...this.data,
      tenantId,
    });
  }

  /**
   * Add baggage item.
   */
  withBaggage(key: string, value: string): TracingContext {
    return new TracingContext({
      ...this.data,
      baggage: {
        ...this.data.baggage,
        [key]: value,
      },
    });
  }

  // ============================================================================
  // CONVERSIONS
  // ============================================================================

  /**
   * Convert to command metadata format.
   */
  toCommandMetadata(): CommandMetadata {
    return {
      correlationId: this.data.correlationId,
      causationId: this.data.causationId,
      actorId: this.data.actorId,
      tenantId: this.data.tenantId,
    };
  }

  /**
   * Convert to HTTP headers for propagation.
   */
  toHeaders(): Record<string, string> {
    const headers: Record<string, string> = {};

    if (this.data.correlationId) {
      headers[HEADER_KEYS.CORRELATION_ID] = this.data.correlationId;
    }
    if (this.data.causationId) {
      headers[HEADER_KEYS.CAUSATION_ID] = this.data.causationId;
    }
    if (this.data.actorId) {
      headers[HEADER_KEYS.ACTOR_ID] = this.data.actorId;
    }
    if (this.data.tenantId) {
      headers[HEADER_KEYS.TENANT_ID] = this.data.tenantId;
    }
    if (this.data.traceparent) {
      headers[HEADER_KEYS.TRACE_PARENT] = this.data.traceparent;
    }
    if (this.data.tracestate) {
      headers[HEADER_KEYS.TRACE_STATE] = this.data.tracestate;
    }

    return headers;
  }

  /**
   * Convert to span attributes for OpenTelemetry.
   */
  toSpanAttributes(): SpanAttributes {
    const attrs: SpanAttributes = {};

    if (this.data.correlationId) {
      attrs['es.correlation_id'] = this.data.correlationId;
    }
    if (this.data.causationId) {
      attrs['es.causation_id'] = this.data.causationId;
    }
    if (this.data.actorId) {
      attrs['es.actor_id'] = this.data.actorId;
    }
    if (this.data.tenantId) {
      attrs['es.tenant_id'] = this.data.tenantId;
    }

    return attrs;
  }

  /**
   * Convert to log context for structured logging.
   */
  toLogContext(): Record<string, string> {
    const context: Record<string, string> = {};

    if (this.data.correlationId) {
      context.correlationId = this.data.correlationId;
    }
    if (this.data.causationId) {
      context.causationId = this.data.causationId;
    }
    if (this.data.actorId) {
      context.actorId = this.data.actorId;
    }
    if (this.data.tenantId) {
      context.tenantId = this.data.tenantId;
    }

    return context;
  }
}

// ============================================================================
// CONTEXT STORAGE
// ============================================================================

/**
 * Async context storage for TracingContext.
 *
 * Uses AsyncLocalStorage when available (Node.js 14.8+) for automatic
 * context propagation through async operations.
 */
// Define the AsyncLocalStorage interface inline to avoid import issues
interface AsyncLocalStorageInterface<T> {
  run<R>(store: T, callback: () => R): R;
  getStore(): T | undefined;
}

class TracingContextStorage {
  private storage: AsyncLocalStorageInterface<TracingContext> | null = null;
  private fallbackContext: TracingContext = TracingContext.empty();

  constructor() {
    // Try to use AsyncLocalStorage if available
    try {
      // Dynamic import to avoid issues in environments without it
      // eslint-disable-next-line @typescript-eslint/no-var-requires, @typescript-eslint/no-require-imports
      const asyncHooks = require('async_hooks');
      if (asyncHooks && asyncHooks.AsyncLocalStorage) {
        this.storage = new asyncHooks.AsyncLocalStorage();
      }
    } catch {
      // AsyncLocalStorage not available, fall back to simple storage
      this.storage = null;
    }
  }

  /**
   * Run a function with a specific context.
   */
  run<T>(context: TracingContext, fn: () => T): T {
    if (this.storage) {
      return this.storage.run(context, fn);
    }
    // Fallback: set context, run, restore
    const previous = this.fallbackContext;
    this.fallbackContext = context;
    try {
      return fn();
    } finally {
      this.fallbackContext = previous;
    }
  }

  /**
   * Get the current context.
   */
  getContext(): TracingContext {
    if (this.storage) {
      return this.storage.getStore() ?? TracingContext.empty();
    }
    return this.fallbackContext;
  }

  /**
   * Check if running in a context.
   */
  hasContext(): boolean {
    if (this.storage) {
      return this.storage.getStore() !== undefined;
    }
    return !this.fallbackContext.isEmpty();
  }
}

/**
 * Global context storage instance.
 */
export const contextStorage = new TracingContextStorage();

/**
 * Run a function with a tracing context.
 *
 * @example
 * ```typescript
 * const ctx = TracingContext.fromRequest(req);
 *
 * await runWithContext(ctx, async () => {
 *   // All operations here will have access to ctx
 *   const current = getCurrentContext();
 *   console.log(current.correlationId);
 * });
 * ```
 */
export function runWithContext<T>(context: TracingContext, fn: () => T): T {
  return contextStorage.run(context, fn);
}

/**
 * Get the current tracing context.
 *
 * Returns an empty context if not running within runWithContext.
 */
export function getCurrentContext(): TracingContext {
  return contextStorage.getContext();
}

/**
 * Check if currently running within a tracing context.
 */
export function hasContext(): boolean {
  return contextStorage.hasContext();
}
