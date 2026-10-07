/**
 * Event definitions and metadata handling for the event sourcing SDK
 */

import { UUID, Timestamp, JsonObject, JsonValue, EventType, Version } from '../types/common';
import {
  EventDecodeError,
  EventPayloadError,
  UnknownEventTypeError,
  UnknownEventVersionError,
  UnsupportedContentTypeError,
  UpcastError,
} from './errors';
import { isJsonObject, normalizeEventVersion, type Upcasters } from './upcast';

/** Metadata headers map */
export type MetadataHeaders = Record<string, string>;

/** Arbitrary metadata payload */
export type CustomMetadata = Record<string, JsonValue>;

/** Trait for domain events */
export interface DomainEvent {
  /** Get the event type identifier */
  readonly eventType: EventType;

  /** Get the schema version of this event */
  readonly schemaVersion: number;

  /** Get the event data as a JSON object */
  toJson(): JsonObject;
}

/** Event metadata that accompanies every event */
export interface EventMetadata {
  /** Unique event identifier */
  readonly eventId: UUID;

  /** When the event occurred */
  readonly timestamp: Timestamp;

  /** When the event was recorded by the store */
  readonly recordedTimestamp: Timestamp;

  /** Aggregate nonce (sequence number) when this event was created */
  readonly aggregateNonce: Version;

  /** ID of the aggregate that produced this event */
  readonly aggregateId: string;

  /** Type of the aggregate that produced this event */
  readonly aggregateType: string;

  /** Tenant that owns the aggregate */
  readonly tenantId?: string;

  /** global nonce assigned by the store (matches proto: uint64 global_nonce) */
  readonly globalNonce?: number;

  /** Content type associated with the payload */
  readonly contentType: string;

  /** Optional correlation identifier */
  readonly correlationId?: string;

  /** Optional causation identifier */
  readonly causationId?: string;

  /** Optional actor identifier */
  readonly actorId?: string;

  /** Event headers (often tracing or compression data) */
  readonly headers: MetadataHeaders;

  /** Optional integrity hash of the payload */
  readonly payloadHash?: string;

  /** Additional metadata */
  readonly customMetadata: CustomMetadata;

  /**
   * Event type as stored (set on read, ADR-027). Equals `event.eventType`
   * unless an upcaster renamed the event.
   */
  readonly storedEventType?: string;

  /**
   * Event version as stored (set on read; 0 read as 1, ADR-027). Equals
   * `event.schemaVersion` unless an upcaster ran.
   */
  readonly storedEventVersion?: number;
}

/** Event envelope that wraps a domain event with metadata */
export interface EventEnvelope<TEvent extends DomainEvent = DomainEvent> {
  /** The domain event */
  readonly event: TEvent;

  /** Event metadata */
  readonly metadata: EventMetadata;
}

/** Base class for domain events with common functionality */
export abstract class BaseDomainEvent implements DomainEvent {
  abstract readonly eventType: EventType;
  abstract readonly schemaVersion: number;

  /**
   * Convert the event to a JSON object: the event's own fields only.
   *
   * `eventType` and `schemaVersion` travel in the envelope metadata
   * (ADR-027), so they are not part of the body. Subclasses can override for
   * custom serialization.
   */
  toJson(): JsonObject {
    return stripEnvelopeKeys(JSON.parse(JSON.stringify(this)) as JsonObject);
  }

  /** Create event metadata */
  static createMetadata(params: EventFactoryParams): EventMetadata {
    const nowIso = new Date().toISOString();
    const timestamp = params.eventTimestamp ?? nowIso;
    const recordedTimestamp = params.recordedTimestamp ?? timestamp;
    return {
      eventId: generateUuid(),
      timestamp,
      recordedTimestamp,
      aggregateNonce: params.aggregateNonce,
      aggregateId: params.aggregateId,
      aggregateType: params.aggregateType,
      tenantId: params.tenantId,
      globalNonce: params.globalNonce,
      contentType: params.contentType ?? 'application/json',
      correlationId: params.correlationId,
      causationId: params.causationId,
      actorId: params.actorId,
      headers: { ...(params.headers ?? {}) },
      payloadHash: params.payloadHash,
      customMetadata: { ...(params.customMetadata ?? {}) },
    };
  }

  /** Create an event envelope */
  static envelope<TEvent extends DomainEvent>(
    event: TEvent,
    metadata: EventMetadata
  ): EventEnvelope<TEvent> {
    return {
      event,
      metadata,
    };
  }
}

/** Event factory for creating events with metadata */
export class EventFactory {
  /** Create an event envelope with generated metadata */
  static create<TEvent extends DomainEvent>(
    event: TEvent,
    params: EventFactoryParams
  ): EventEnvelope<TEvent> {
    const metadata = BaseDomainEvent.createMetadata(params);
    return BaseDomainEvent.envelope(event, metadata);
  }
}

/**
 * Keys the TypeScript SDK <= 0.17 wrote into every payload (its event class
 * fields). They duplicate envelope metadata (ADR-027): writers no longer emit
 * them and readers drop them before upcasting and decoding, so payloads
 * already stored still decode. They are `DomainEvent` members, so a TS event
 * can never carry them as data. (`event_type` is NOT dropped: for TS it is
 * an ordinary field name.)
 */
export const ENVELOPE_ECHO_KEYS: readonly string[] = ['eventType', 'schemaVersion'];
const TS_RESERVED_KEYS = ENVELOPE_ECHO_KEYS;

/** The event body without the keys that duplicate envelope metadata. */
export function stripEnvelopeKeys(
  body: JsonObject,
  keys: readonly string[] = TS_RESERVED_KEYS
): JsonObject {
  if (!keys.some((k) => k in body)) return body;
  const out: JsonObject = {};
  for (const [k, v] of Object.entries(body)) {
    if (!keys.includes(k)) out[k] = v;
  }
  return out;
}

/** `application/json` or empty (unset); parameters and case are ignored. */
export function isJsonContentType(contentType: string | undefined | null): boolean {
  const essence = (contentType ?? '').split(';')[0].trim().toLowerCase();
  return essence === '' || essence === 'application/json';
}

/**
 * The `event_version` to write for `event`: its `schemaVersion`, with a
 * missing or 0 version written as 1 (ADR-027: versions start at 1).
 */
export function eventVersionOf(event: DomainEvent): number {
  const v = (event as { schemaVersion?: unknown }).schemaVersion;
  if (v === undefined || v === null || v === 0) return 1;
  if (typeof v !== 'number' || !Number.isInteger(v) || v < 1) {
    throw new Error(
      `${event.eventType}: schemaVersion must be an integer >= 1 (ADR-027), got ${String(v)}`
    );
  }
  return v;
}

/** The payload to write for `event`: its own fields only (ADR-027). */
export function encodeEventPayload(event: DomainEvent): JsonObject {
  const body: unknown = event.toJson();
  if (!isJsonObject(body)) {
    throw new Error(`${event.eventType}: toJson() must return a JSON object (ADR-027)`);
  }
  return stripEnvelopeKeys(body);
}

/** A stored event to decode. */
export interface StoredEvent {
  eventType: string;
  /** As stored; 0 (proto3 unset) is read as 1. */
  eventVersion?: number;
  /** Parsed JSON payload. */
  payload: unknown;
  contentType?: string;
}

/** Options for decoding. */
export interface DecodeOptions {
  /** Steps that migrate stored events to registered versions first. */
  upcasters?: Upcasters;
  /** Store position, reported on errors. */
  globalNonce?: number;
  /**
   * Throw `UnknownEventTypeError` for a type with no registered class,
   * instead of returning a generic event (ADR-023).
   */
  requireRegistered?: boolean;
}

/** Result of `decodeEvent`. */
export interface DecodedEvent {
  event: DomainEvent;
  /** What `event` was decoded as, after upcasting. */
  eventType: string;
  eventVersion: number;
  /** As stored. */
  storedEventType: string;
  storedEventVersion: number;
}

/**
 * Decode a stored event (ADR-027 reading steps).
 *
 * 1. Content type must be empty or `application/json`.
 * 2. Version 0 is read as 1.
 * 3. The payload must be a JSON object; `ENVELOPE_ECHO_KEYS` are dropped.
 * 4. The upcaster chain runs on `(eventType, eventVersion, payload)`.
 * 5. The class registered for the final `(eventType, eventVersion)` is
 *    instantiated and the payload assigned to it.
 *
 * Errors are typed (`EventDecodeError` subclasses), never skipped. A type
 * registered at other versions only is `UnknownEventVersionError`. A type
 * with no registered class returns a generic event carrying the type,
 * version and every payload field (ADR-023), unless `requireRegistered`.
 */
export function decodeEvent(stored: StoredEvent, options: DecodeOptions = {}): DecodedEvent {
  const storedType = stored.eventType ?? '';
  const storedVersion = normalizeEventVersion(stored.eventVersion);
  const globalNonce = options.globalNonce ?? 0;
  if (!isJsonContentType(stored.contentType)) {
    throw new UnsupportedContentTypeError(
      storedType,
      storedVersion,
      `content type '${stored.contentType}' is not JSON`,
      globalNonce
    );
  }
  if (!isJsonObject(stored.payload)) {
    throw new EventPayloadError(
      storedType,
      storedVersion,
      'payload is not a JSON object',
      globalNonce
    );
  }
  let body = stripEnvelopeKeys(stored.payload, ENVELOPE_ECHO_KEYS);
  let ty = storedType;
  let version = storedVersion;
  if (options.upcasters?.handles(ty, version)) {
    try {
      ({
        eventType: ty,
        eventVersion: version,
        payload: body,
      } = options.upcasters.upcast(ty, version, body));
    } catch (err) {
      if (err instanceof UpcastError) {
        throw new UpcastError(err.eventType, err.eventVersion, err.reason, globalNonce, err.cause);
      }
      throw err;
    }
    body = stripEnvelopeKeys(body, ENVELOPE_ECHO_KEYS);
  }
  const decoded = (event: DomainEvent): DecodedEvent => ({
    event,
    eventType: ty,
    eventVersion: version,
    storedEventType: storedType,
    storedEventVersion: storedVersion,
  });

  const EventClass = ty ? EventSerializer.resolveEventClass(ty, version) : undefined;
  if (!EventClass) {
    const known = ty ? EventSerializer.registeredVersions(ty) : [];
    if (known.length > 0) {
      throw new UnknownEventVersionError(
        ty,
        version,
        `registered versions are [${known.join(', ')}]; register a class for v${version} ` +
          `or an upcaster from v${version}`,
        globalNonce
      );
    }
    if (options.requireRegistered) {
      throw new UnknownEventTypeError(ty, version, 'no event class registered', globalNonce);
    }
    const payloadObject = body;
    const generic: DomainEvent = {
      ...(payloadObject as object),
      eventType: ty,
      schemaVersion: version,
      toJson: () => payloadObject,
    };
    return decoded(generic);
  }

  let event: DomainEvent;
  try {
    event = new EventClass();
  } catch (err) {
    throw new EventDecodeError(
      ty,
      version,
      `cannot construct ${EventClass.name} without arguments: ${(err as Error).message}`,
      globalNonce,
      err as Error
    );
  }
  Object.assign(event, body);
  // A class without a version (or 0) is v1 on the wire; report it as such.
  const own = (event as { schemaVersion?: unknown }).schemaVersion;
  if (own === undefined || own === null || own === 0) {
    Object.defineProperty(event, 'schemaVersion', { value: version, enumerable: false });
  }
  return decoded(event);
}

interface RegisteredEvent {
  ctor: new () => DomainEvent;
  /** Explicit, or resolved on first lookup. */
  version?: number;
}

/** Event serializer for converting events to/from JSON */
export class EventSerializer {
  /** event type -> registrations, in registration order (later wins). */
  private static readonly eventRegistry = new Map<EventType, RegisteredEvent[]>();

  /**
   * Register an event class for deserialization, keyed by
   * `(eventType, version)` (ADR-027).
   *
   * `version` defaults to the class's `schemaVersion`, read from an instance
   * on first lookup (decoding already requires a no-argument constructor);
   * if that fails, the major number of its `@Event` version string; else 1.
   * A later registration of the same pair overwrites the earlier one.
   */
  static registerEvent<TEvent extends DomainEvent>(
    eventType: EventType,
    eventClass: new () => TEvent,
    version?: number
  ): void {
    if (version !== undefined && (!Number.isInteger(version) || version < 1)) {
      throw new Error(`event version for '${eventType}' must be an integer >= 1`);
    }
    const entries = this.eventRegistry.get(eventType) ?? [];
    entries.push({ ctor: eventClass, version });
    this.eventRegistry.set(eventType, entries);
  }

  /** The class registered for `(eventType, version)`, if any. */
  static resolveEventClass(
    eventType: EventType,
    version: number
  ): (new () => DomainEvent) | undefined {
    const entries = this.eventRegistry.get(eventType);
    if (!entries) return undefined;
    for (let i = entries.length - 1; i >= 0; i -= 1) {
      if (registeredVersion(entries[i]) === version) return entries[i].ctor;
    }
    return undefined;
  }

  /** The versions registered for `eventType`, ascending. */
  static registeredVersions(eventType: EventType): number[] {
    const entries = this.eventRegistry.get(eventType) ?? [];
    return [...new Set(entries.map(registeredVersion))].sort((a, b) => a - b);
  }

  /** Serialize an event envelope to JSON */
  static serialize<TEvent extends DomainEvent>(envelope: EventEnvelope<TEvent>): JsonObject {
    return {
      event: {
        eventType: envelope.event.eventType,
        schemaVersion: eventVersionOf(envelope.event),
        data: encodeEventPayload(envelope.event),
      },
      metadata: metadataToJson(envelope.metadata),
    };
  }

  /**
   * Deserialize an event envelope from JSON, decoding by
   * `(eventType, schemaVersion)` (see `decodeEvent`).
   */
  static deserialize(json: JsonObject, options: DecodeOptions = {}): EventEnvelope {
    const eventData = json.event as JsonObject;
    const metadata = metadataFromJson(json.metadata as JsonObject);
    const decoded = decodeEvent(
      {
        eventType: String(eventData.eventType ?? ''),
        eventVersion: Number(eventData.schemaVersion ?? 0),
        payload: eventData.data,
        contentType: metadata.contentType,
      },
      { globalNonce: metadata.globalNonce ?? undefined, ...options }
    );
    return {
      event: decoded.event,
      metadata: {
        ...metadata,
        storedEventType: decoded.storedEventType,
        storedEventVersion: decoded.storedEventVersion,
      },
    };
  }
}

function registeredVersion(entry: RegisteredEvent): number {
  if (entry.version === undefined) {
    entry.version = probeVersion(entry.ctor);
  }
  return entry.version;
}

function probeVersion(ctor: new () => DomainEvent): number {
  try {
    const v = (new ctor() as { schemaVersion?: unknown }).schemaVersion;
    if (v === undefined || v === null || v === 0) return 1;
    if (typeof v === 'number' && Number.isInteger(v) && v >= 1) return v;
  } catch {
    // Constructor needs arguments: fall back to the decorator.
  }
  const declared = (ctor as EventAwareConstructor)[EVENT_METADATA]?.version;
  const major = declared ? Number(/^v?(\d+)/.exec(declared)?.[1]) : NaN;
  return Number.isInteger(major) && major >= 1 ? major : 1;
}

/** Parameters required to construct metadata for an event */
export interface EventFactoryParams {
  aggregateId: string;
  aggregateType: string;
  aggregateNonce: Version;
  tenantId?: string;
  globalNonce?: number;
  contentType?: string;
  correlationId?: string;
  causationId?: string;
  actorId?: string;
  headers?: MetadataHeaders;
  payloadHash?: string;
  eventTimestamp?: Timestamp;
  recordedTimestamp?: Timestamp;
  customMetadata?: CustomMetadata;
}

function generateUuid(): UUID {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }

  // Simple fallback (RFC4122 variant approximation)
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
    const r = (Math.random() * 16) | 0;
    const v = c === 'x' ? r : (r & 0x3) | 0x8;
    return v.toString(16);
  }) as UUID;
}

function metadataToJson(metadata: EventMetadata): Record<string, JsonValue> {
  return {
    ...metadata,
    headers: { ...metadata.headers },
    customMetadata: { ...metadata.customMetadata },
  } as Record<string, JsonValue>;
}

function metadataFromJson(json: JsonObject): EventMetadata {
  const baseTimestamp = (json.timestamp as string | undefined) ?? new Date().toISOString();
  const recordedTimestamp =
    (json.recordedTimestamp as string | undefined) ?? baseTimestamp ?? new Date().toISOString();

  return {
    eventId: (json.eventId as UUID) ?? generateUuid(),
    timestamp: baseTimestamp,
    recordedTimestamp,
    aggregateNonce: Number(json.aggregateNonce ?? json.aggregateVersion ?? 0) as Version,
    aggregateId: String(json.aggregateId ?? ''),
    aggregateType: String(json.aggregateType ?? ''),
    tenantId: json.tenantId === undefined ? undefined : String(json.tenantId),
    globalNonce: json.globalNonce === undefined ? undefined : Number(json.globalNonce as number),
    contentType: String(json.contentType ?? 'application/json'),
    correlationId:
      json.correlationId === undefined ? undefined : String(json.correlationId as string),
    causationId: json.causationId === undefined ? undefined : String(json.causationId as string),
    actorId: json.actorId === undefined ? undefined : String(json.actorId as string),
    headers: normalizeHeaders(json.headers),
    payloadHash: json.payloadHash === undefined ? undefined : String(json.payloadHash),
    customMetadata: normalizeCustomMetadata(json.customMetadata),
  };
}

function normalizeHeaders(value: unknown): MetadataHeaders {
  if (!value || typeof value !== 'object') {
    return {};
  }

  const headers: MetadataHeaders = {};
  for (const [key, val] of Object.entries(value as Record<string, unknown>)) {
    if (typeof val === 'string' || typeof val === 'number' || typeof val === 'boolean') {
      headers[key] = String(val);
    }
  }
  return headers;
}

function normalizeCustomMetadata(value: unknown): CustomMetadata {
  if (!value || typeof value !== 'object') {
    return {};
  }

  const metadata: CustomMetadata = {};
  for (const [key, val] of Object.entries(value as Record<string, JsonValue>)) {
    metadata[key] = val as JsonValue;
  }
  return metadata;
}

// ============================================================================
// EVENT DECORATOR (ADR-010)
// ============================================================================

/** Event metadata storage symbol */
export const EVENT_METADATA: unique symbol = Symbol('eventMetadata');

/** Event metadata */
export interface EventDecoratorMetadata {
  eventType: EventType;
  version: string;
}

/** Type-aware constructor with event metadata */
export type EventAwareConstructor = {
  [EVENT_METADATA]?: EventDecoratorMetadata;
};

/**
 * Validate event version format.
 * Supports two formats:
 * - Simple: "v1", "v2", "v3", etc. (v followed by integer)
 * - Semantic: "1.0.0", "2.1.3", etc. (major.minor.patch)
 *
 * @param version - The version string to validate
 * @returns True if valid, false otherwise
 */
function isValidEventVersion(version: string): boolean {
  // Simple version format: v1, v2, v3, etc.
  const simpleVersionRegex = /^v\d+$/;
  if (simpleVersionRegex.test(version)) {
    return true;
  }

  // Semantic version format: 1.0.0, 2.1.3, etc.
  const semanticVersionRegex = /^\d+\.\d+\.\d+$/;
  if (semanticVersionRegex.test(version)) {
    return true;
  }

  return false;
}

/**
 * Decorator for event classes to store metadata about event type and version.
 * This enables the VSA CLI to discover and validate events automatically.
 *
 * @param eventType - The event type identifier (e.g., "TaskCreated")
 * @param version - The event version. Must be either:
 *                  - Simple format: "v1", "v2", "v3", etc. (recommended)
 *                  - Semantic format: "1.0.0", "2.1.3", etc. (advanced)
 *                  The class's `schemaVersion` is what is written as `event_version`
 *                  and what the class is registered (and decoded) under (ADR-027);
 *                  keep the two in step. The string is only used when an
 *                  instance cannot be constructed without arguments.
 *
 * @throws {Error} If version format is invalid
 *
 * @example Simple versioning (recommended):
 * ```typescript
 * @Event("TaskCreated", "v1")
 * export class TaskCreatedEvent extends BaseDomainEvent {
 *   readonly eventType = "TaskCreated" as const;
 *   readonly schemaVersion = 1 as const;
 *   // ...
 * }
 * ```
 *
 * @example Semantic versioning (advanced):
 * ```typescript
 * @Event("TaskCreated", "2.0.0")
 * export class TaskCreatedEventV2 extends BaseDomainEvent {
 *   readonly eventType = "TaskCreated" as const;
 *   readonly schemaVersion = 2 as const;
 *   // ...
 * }
 * ```
 *
 * @see ADR-007: Event Versioning and Upcasters
 * @see ADR-010: Decorator Patterns for Framework Integration
 */
export function Event(eventType: EventType, version: string) {
  return function <T extends new (...args: any[]) => DomainEvent>(constructor: T): T {
    // Validate version format
    if (!isValidEventVersion(version)) {
      throw new Error(
        `Invalid event version format: "${version}" for event "${eventType}". ` +
          `Version must be either simple format (e.g., "v1", "v2") or semantic format (e.g., "1.0.0", "2.1.3"). ` +
          `See ADR-007 for event versioning guidelines.`
      );
    }

    // Store metadata on the constructor
    (constructor as EventAwareConstructor)[EVENT_METADATA] = {
      eventType,
      version,
    };

    // ADR-023: Auto-register in the event type registry for deserialization
    EventSerializer.registerEvent(eventType, constructor as unknown as new () => DomainEvent);

    return constructor;
  };
}

/**
 * Get event metadata from an event class
 */
export function getEventMetadata(
  eventClass: EventAwareConstructor
): EventDecoratorMetadata | undefined {
  return eventClass[EVENT_METADATA];
}
