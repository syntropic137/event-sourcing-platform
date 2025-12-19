/**
 * Structured Logger
 *
 * A simple structured logger with context enrichment and multiple output targets.
 */

import { LogLevel, LogContext, LogEntry, Logger, LogOutput } from '../types';
import { getCurrentContext } from '../tracing/tracing-context';

// ============================================================================
// LOG LEVEL UTILITIES
// ============================================================================

/**
 * Numeric priority for log levels (higher = more severe).
 */
const LOG_LEVEL_PRIORITY: Record<LogLevel, number> = {
  trace: 0,
  debug: 1,
  info: 2,
  warn: 3,
  error: 4,
  fatal: 5,
};

/**
 * Check if a log level should be output given the minimum level.
 */
function shouldLog(level: LogLevel, minLevel: LogLevel): boolean {
  return LOG_LEVEL_PRIORITY[level] >= LOG_LEVEL_PRIORITY[minLevel];
}

// ============================================================================
// LOG OUTPUTS
// ============================================================================

/**
 * Console output that writes JSON to stdout/stderr.
 */
export class ConsoleJsonOutput implements LogOutput {
  write(entry: LogEntry): void {
    const output = JSON.stringify(entry);

    if (entry.level === 'error' || entry.level === 'fatal') {
      console.error(output);
    } else {
      console.log(output);
    }
  }
}

/**
 * Console output with human-readable formatting.
 */
export class ConsolePrettyOutput implements LogOutput {
  private readonly colors: Record<LogLevel, string> = {
    trace: '\x1b[90m', // Gray
    debug: '\x1b[36m', // Cyan
    info: '\x1b[32m', // Green
    warn: '\x1b[33m', // Yellow
    error: '\x1b[31m', // Red
    fatal: '\x1b[35m', // Magenta
  };

  private readonly reset = '\x1b[0m';

  write(entry: LogEntry): void {
    const color = this.colors[entry.level];
    const time = new Date(entry.timestamp).toISOString().substring(11, 23);
    const level = entry.level.toUpperCase().padEnd(5);
    const component = entry.component ? `[${entry.component}] ` : '';
    const correlation = entry.correlationId ? ` (${entry.correlationId.substring(0, 8)})` : '';

    // Build context string
    const contextKeys = Object.keys(entry).filter(
      (k) =>
        !['timestamp', 'level', 'message', 'component', 'correlationId', 'causationId', 'actorId'].includes(k)
    );
    const contextStr =
      contextKeys.length > 0
        ? ` ${JSON.stringify(
          Object.fromEntries(contextKeys.map((k) => [k, entry[k]]))
        )}`
        : '';

    const output = `${color}${time} ${level}${this.reset} ${component}${entry.message}${correlation}${contextStr}`;

    if (entry.level === 'error' || entry.level === 'fatal') {
      console.error(output);
    } else {
      console.log(output);
    }
  }
}

/**
 * No-op output for testing or disabling logs.
 */
export class NoOpOutput implements LogOutput {
  write(_entry: LogEntry): void {
    // No-op
  }
}

/**
 * Output that collects log entries for testing.
 */
export class CollectorOutput implements LogOutput {
  public readonly entries: LogEntry[] = [];

  write(entry: LogEntry): void {
    this.entries.push(entry);
  }

  clear(): void {
    this.entries.length = 0;
  }

  findByMessage(message: string): LogEntry | undefined {
    return this.entries.find((e) => e.message.includes(message));
  }

  findByLevel(level: LogLevel): LogEntry[] {
    return this.entries.filter((e) => e.level === level);
  }
}

// ============================================================================
// STRUCTURED LOGGER IMPLEMENTATION
// ============================================================================

/**
 * Options for creating a structured logger.
 */
export interface StructuredLoggerOptions {
  /** Minimum log level to output (default: 'info') */
  minLevel?: LogLevel;

  /** Output target (default: ConsoleJsonOutput) */
  output?: LogOutput;

  /** Component/logger name */
  component?: string;

  /** Base context added to all log entries */
  baseContext?: LogContext;

  /** Whether to automatically enrich from tracing context (default: true) */
  enrichFromContext?: boolean;
}

/**
 * Structured logger implementation.
 *
 * Features:
 * - JSON structured logging
 * - Automatic context enrichment from TracingContext
 * - Child loggers with additional context
 * - Configurable log levels
 *
 * @example
 * ```typescript
 * const logger = new StructuredLogger({ component: 'OrderService' });
 *
 * logger.info('Order placed', { orderId: '123', total: 150.00 });
 * // Output: {"timestamp":"...","level":"info","message":"Order placed","component":"OrderService","orderId":"123","total":150}
 * ```
 */
export class StructuredLogger implements Logger {
  private readonly minLevel: LogLevel;
  private readonly output: LogOutput;
  private readonly component?: string;
  private readonly baseContext: LogContext;
  private readonly enrichFromContext: boolean;

  constructor(options: StructuredLoggerOptions = {}) {
    this.minLevel = options.minLevel ?? 'info';
    this.output = options.output ?? new ConsoleJsonOutput();
    this.component = options.component;
    this.baseContext = options.baseContext ?? {};
    this.enrichFromContext = options.enrichFromContext ?? true;
  }

  private log(level: LogLevel, message: string, context?: LogContext): void {
    if (!shouldLog(level, this.minLevel)) {
      return;
    }

    // Build log entry
    const entry: LogEntry = {
      timestamp: new Date().toISOString(),
      level,
      message,
    };

    // Add component
    if (this.component) {
      entry.component = this.component;
    }

    // Add tracing context
    if (this.enrichFromContext) {
      const tracingCtx = getCurrentContext();
      if (tracingCtx.correlationId) {
        entry.correlationId = tracingCtx.correlationId;
      }
      if (tracingCtx.causationId) {
        entry.causationId = tracingCtx.causationId;
      }
      if (tracingCtx.actorId) {
        entry.actorId = tracingCtx.actorId;
      }
    }

    // Add base context
    for (const [key, value] of Object.entries(this.baseContext)) {
      entry[key] = value;
    }

    // Add call-site context
    if (context) {
      for (const [key, value] of Object.entries(context)) {
        entry[key] = value;
      }
    }

    this.output.write(entry);
  }

  trace(message: string, context?: LogContext): void {
    this.log('trace', message, context);
  }

  debug(message: string, context?: LogContext): void {
    this.log('debug', message, context);
  }

  info(message: string, context?: LogContext): void {
    this.log('info', message, context);
  }

  warn(message: string, context?: LogContext): void {
    this.log('warn', message, context);
  }

  error(message: string, context?: LogContext): void {
    this.log('error', message, context);
  }

  fatal(message: string, context?: LogContext): void {
    this.log('fatal', message, context);
  }

  /**
   * Create a child logger with additional context.
   */
  child(context: LogContext): Logger {
    return new StructuredLogger({
      minLevel: this.minLevel,
      output: this.output,
      component: this.component,
      baseContext: { ...this.baseContext, ...context },
      enrichFromContext: this.enrichFromContext,
    });
  }

  /**
   * Create a child logger for a component.
   */
  forComponent(component: string): Logger {
    return new StructuredLogger({
      minLevel: this.minLevel,
      output: this.output,
      component,
      baseContext: this.baseContext,
      enrichFromContext: this.enrichFromContext,
    });
  }
}

// ============================================================================
// GLOBAL LOGGER
// ============================================================================

let globalLogger: Logger = new StructuredLogger();

/**
 * Get the global logger.
 */
export function getGlobalLogger(): Logger {
  return globalLogger;
}

/**
 * Set the global logger.
 */
export function setGlobalLogger(logger: Logger): void {
  globalLogger = logger;
}

/**
 * Create and set a global logger with the specified options.
 */
export function configureGlobalLogger(options: StructuredLoggerOptions): Logger {
  const logger = new StructuredLogger(options);
  setGlobalLogger(logger);
  return logger;
}

/**
 * Reset to default logger.
 */
export function resetGlobalLogger(): void {
  globalLogger = new StructuredLogger();
}

// ============================================================================
// CONVENIENCE FUNCTIONS
// ============================================================================

/**
 * Log at trace level using the global logger.
 */
export function trace(message: string, context?: LogContext): void {
  globalLogger.trace(message, context);
}

/**
 * Log at debug level using the global logger.
 */
export function debug(message: string, context?: LogContext): void {
  globalLogger.debug(message, context);
}

/**
 * Log at info level using the global logger.
 */
export function info(message: string, context?: LogContext): void {
  globalLogger.info(message, context);
}

/**
 * Log at warn level using the global logger.
 */
export function warn(message: string, context?: LogContext): void {
  globalLogger.warn(message, context);
}

/**
 * Log at error level using the global logger.
 */
export function error(message: string, context?: LogContext): void {
  globalLogger.error(message, context);
}

/**
 * Log at fatal level using the global logger.
 */
export function fatal(message: string, context?: LogContext): void {
  globalLogger.fatal(message, context);
}

/**
 * Create a logger for a specific component.
 */
export function forComponent(component: string): Logger {
  return globalLogger.forComponent(component);
}

// ============================================================================
// ES-SPECIFIC LOG HELPERS
// ============================================================================

/**
 * Standard log messages for ES operations.
 *
 * Follows the log level conventions from ADR-017.
 */
export const ESLogMessages = {
  // Command operations
  commandReceived: (commandType: string, aggregateId: string) => ({
    message: `Command received: ${commandType}`,
    context: { commandType, aggregateId },
    level: 'debug' as LogLevel,
  }),

  commandExecuted: (commandType: string, aggregateId: string) => ({
    message: `Command executed: ${commandType}`,
    context: { commandType, aggregateId },
    level: 'info' as LogLevel,
  }),

  commandFailed: (commandType: string, aggregateId: string, error: Error) => ({
    message: `Command failed: ${commandType}`,
    context: {
      commandType,
      aggregateId,
      errorType: error.constructor.name,
      errorMessage: error.message,
    },
    level: 'error' as LogLevel,
  }),

  // Event operations
  eventAppended: (eventType: string, aggregateId: string) => ({
    message: `Event appended: ${eventType}`,
    context: { eventType, aggregateId },
    level: 'debug' as LogLevel,
  }),

  // Aggregate operations
  aggregateLoaded: (aggregateType: string, aggregateId: string, version: number) => ({
    message: `Aggregate loaded: ${aggregateType}`,
    context: { aggregateType, aggregateId, version },
    level: 'debug' as LogLevel,
  }),

  aggregateNotFound: (aggregateType: string, aggregateId: string) => ({
    message: `Aggregate not found: ${aggregateType}`,
    context: { aggregateType, aggregateId },
    level: 'warn' as LogLevel,
  }),

  // Projection operations
  projectionEventProcessed: (projectionName: string, eventType: string) => ({
    message: `Projection processed event: ${projectionName}`,
    context: { projectionName, eventType },
    level: 'debug' as LogLevel,
  }),

  projectionEventRetrying: (
    projectionName: string,
    eventType: string,
    attempt: number
  ) => ({
    message: `Projection retrying: ${projectionName}`,
    context: { projectionName, eventType, attempt },
    level: 'warn' as LogLevel,
  }),

  projectionEventFailed: (
    projectionName: string,
    eventType: string,
    error: Error
  ) => ({
    message: `Projection failed (sent to DLQ): ${projectionName}`,
    context: {
      projectionName,
      eventType,
      errorType: error.constructor.name,
      errorMessage: error.message,
    },
    level: 'error' as LogLevel,
  }),

  projectionCheckpointSaved: (projectionName: string, position: number | bigint) => ({
    message: `Projection checkpoint saved: ${projectionName}`,
    context: { projectionName, position: position.toString() },
    level: 'debug' as LogLevel,
  }),
};
