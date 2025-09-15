/**
 * Common types used throughout the event sourcing SDK
 */

/** UUID string type */
export type UUID = string;

/** Timestamp in ISO 8601 format */
export type Timestamp = string;

/** JSON-serializable value */
export type JsonValue = string | number | boolean | null | JsonObject | JsonArray;

/** JSON object */
export interface JsonObject {
  [key: string]: JsonValue;
}

/** JSON array */
export type JsonArray = JsonValue[];

/** Event stream name */
export type StreamName = string;

/** Aggregate ID */
export type AggregateId = string;

/** Event type identifier */
export type EventType = string;

/** Version number for optimistic concurrency control */
export type Version = number;

/** Error types that can occur in the event sourcing system */
export interface EventSourcingError extends Error {
  readonly code: string;
  readonly details?: Record<string, unknown>;
}

/** Base configuration for SDK components */
export interface BaseConfig {
  /** Event store server address */
  eventStoreUrl: string;

  /** Connection timeout in milliseconds */
  timeoutMs?: number;

  /** Enable debug logging */
  debug?: boolean;
}
