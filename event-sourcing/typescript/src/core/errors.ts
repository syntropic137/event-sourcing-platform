/**
 * Error types for the event sourcing SDK
 */

import { EventSourcingError } from '../types/common';

/** Base class for all event sourcing errors */
export abstract class BaseEventSourcingError extends Error implements EventSourcingError {
  abstract readonly code: string;
  readonly details?: Record<string, unknown>;

  constructor(message: string, details?: Record<string, unknown>) {
    super(message);
    this.name = this.constructor.name;
    this.details = details;

    // Ensure the error stack trace points to where the error was thrown
    if (Error.captureStackTrace) {
      Error.captureStackTrace(this, this.constructor);
    }
  }
}

/** Aggregate not found error */
export class AggregateNotFoundError extends BaseEventSourcingError {
  readonly code = 'AGGREGATE_NOT_FOUND';

  constructor(aggregateType: string, aggregateId: string) {
    super(`Aggregate not found: ${aggregateType}:${aggregateId}`, {
      aggregateType,
      aggregateId,
    });
  }
}

/** Concurrency conflict error */
export class ConcurrencyConflictError extends BaseEventSourcingError {
  readonly code = 'CONCURRENCY_CONFLICT';

  constructor(expectedAggregateNonce: number, actualAggregateNonce: number) {
    super(`Concurrency conflict: expected aggregate nonce ${expectedAggregateNonce}, got ${actualAggregateNonce}`, {
      expectedAggregateNonce,
      actualAggregateNonce,
    });
  }
}

/** Invalid aggregate state error */
export class InvalidAggregateStateError extends BaseEventSourcingError {
  readonly code = 'INVALID_AGGREGATE_STATE';

  constructor(aggregateType: string, reason: string) {
    super(`Invalid aggregate state for ${aggregateType}: ${reason}`, {
      aggregateType,
      reason,
    });
  }
}

/** Command validation error */
export class CommandValidationError extends BaseEventSourcingError {
  readonly code = 'COMMAND_VALIDATION_ERROR';

  constructor(commandType: string, validationErrors: string[]) {
    super(`Command validation failed for ${commandType}: ${validationErrors.join(', ')}`, {
      commandType,
      validationErrors,
    });
  }
}

/** Event store communication error */
export class EventStoreError extends BaseEventSourcingError {
  readonly code = 'EVENT_STORE_ERROR';

  constructor(message: string, originalError?: Error) {
    super(`Event store error: ${message}`, {
      originalError: originalError?.message,
      originalStack: originalError?.stack,
    });
  }
}

/** Serialization error */
export class SerializationError extends BaseEventSourcingError {
  readonly code = 'SERIALIZATION_ERROR';

  constructor(operation: 'serialize' | 'deserialize', dataType: string, originalError?: Error) {
    super(`Failed to ${operation} ${dataType}`, {
      operation,
      dataType,
      originalError: originalError?.message,
    });
  }
}
