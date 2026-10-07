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
    super(
      `Concurrency conflict: expected aggregate nonce ${expectedAggregateNonce}, got ${actualAggregateNonce}`,
      {
        expectedAggregateNonce,
        actualAggregateNonce,
      }
    );
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
  readonly code: 'EVENT_STORE_ERROR' | 'EVENT_STORE_UNAUTHENTICATED' = 'EVENT_STORE_ERROR';

  constructor(message: string, originalError?: Error) {
    super(`Event store error: ${message}`, {
      originalError: originalError?.message,
      originalStack: originalError?.stack,
    });
  }
}

/**
 * The event store (or the ADR-024 gateway in front of it) rejected the
 * client's credentials: gRPC `UNAUTHENTICATED`. Retrying without new
 * credentials will not help. Never contains the credentials.
 */
export class EventStoreAuthenticationError extends EventStoreError {
  override readonly code = 'EVENT_STORE_UNAUTHENTICATED' as const;
}

/** gRPC status code UNAUTHENTICATED. */
const GRPC_UNAUTHENTICATED = 16;

/** True for a gRPC `UNAUTHENTICATED` error. */
export function isUnauthenticatedError(err: unknown): boolean {
  return (err as { code?: unknown } | null)?.code === GRPC_UNAUTHENTICATED;
}

/**
 * Wrap a failed event store call: {@link EventStoreAuthenticationError} for
 * gRPC `UNAUTHENTICATED`, {@link EventStoreError} otherwise.
 */
export function toEventStoreError(message: string, err: unknown): EventStoreError {
  const original = err as Error;
  return isUnauthenticatedError(err)
    ? new EventStoreAuthenticationError(message, original)
    : new EventStoreError(message, original);
}

/**
 * A stored event cannot be decoded by this reader (ADR-027).
 *
 * Thrown on read instead of handing the event to code written for another
 * schema; never skipped. Retrying does not help: fix it in code (register the
 * event class or an upcaster). `eventType`/`eventVersion` are as stored
 * (version 0 read as 1), or as produced by the upcaster chain when a later
 * stage failed. `globalNonce` is the event's store position (0 if unknown).
 */
export class EventDecodeError extends BaseEventSourcingError {
  readonly code: string = 'EVENT_DECODE_ERROR';
  readonly eventType: string;
  readonly eventVersion: number;
  readonly reason: string;
  readonly globalNonce: number;
  readonly cause?: Error;

  constructor(
    eventType: string,
    eventVersion: number,
    reason: string,
    globalNonce = 0,
    cause?: Error
  ) {
    super(`Cannot decode event '${eventType}' v${eventVersion}: ${reason}`, {
      eventType,
      eventVersion,
      globalNonce,
      originalError: cause?.message,
    });
    this.eventType = eventType;
    this.eventVersion = eventVersion;
    this.reason = reason;
    this.globalNonce = globalNonce;
    this.cause = cause;
  }
}

/** No event class is registered for the type (strict decode only; the gRPC
 * adapter returns an unregistered type as a generic event, ADR-023). */
export class UnknownEventTypeError extends EventDecodeError {
  readonly code: string = 'UNKNOWN_EVENT_TYPE';
}

/** The type is registered, but not at this version, and no upcaster maps the
 * stored version to a registered one. */
export class UnknownEventVersionError extends EventDecodeError {
  readonly code: string = 'UNKNOWN_EVENT_VERSION';
}

/** The payload is not JSON, or not a JSON object. */
export class EventPayloadError extends EventDecodeError {
  readonly code: string = 'EVENT_PAYLOAD_ERROR';
}

/** The stored content type is neither empty nor `application/json`. */
export class UnsupportedContentTypeError extends EventDecodeError {
  readonly code: string = 'UNSUPPORTED_CONTENT_TYPE';
}

/** An upcaster step threw or did not return a JSON object, or the chain cycled. */
export class UpcastError extends EventDecodeError {
  readonly code: string = 'UPCAST_ERROR';
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
