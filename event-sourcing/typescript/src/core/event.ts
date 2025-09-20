/**
 * Event definitions and metadata handling for the event sourcing SDK
 */

import { UUID, Timestamp, JsonObject, JsonValue, EventType, Version } from '../types/common';

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

  /** Version of the aggregate when this event was created */
  readonly aggregateVersion: Version;

  /** ID of the aggregate that produced this event */
  readonly aggregateId: string;

  /** Type of the aggregate that produced this event */
  readonly aggregateType: string;

  /** Tenant that owns the aggregate */
  readonly tenantId?: string;

  /** Global position assigned by the store */
  readonly globalPosition?: number;

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

  /** Convert the event to a JSON object */
  toJson(): JsonObject {
    // Default implementation uses JSON serialization
    // Subclasses can override for custom serialization
    return JSON.parse(JSON.stringify(this));
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
      aggregateVersion: params.aggregateVersion,
      aggregateId: params.aggregateId,
      aggregateType: params.aggregateType,
      tenantId: params.tenantId,
      globalPosition: params.globalPosition,
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

/** Event serializer for converting events to/from JSON */
export class EventSerializer {
  private static readonly eventRegistry = new Map<EventType, new () => DomainEvent>();

  /** Register an event class for deserialization */
  static registerEvent<TEvent extends DomainEvent>(
    eventType: EventType,
    eventClass: new () => TEvent
  ): void {
    this.eventRegistry.set(eventType, eventClass);
  }

  /** Serialize an event envelope to JSON */
  static serialize<TEvent extends DomainEvent>(envelope: EventEnvelope<TEvent>): JsonObject {
    return {
      event: {
        eventType: envelope.event.eventType,
        schemaVersion: envelope.event.schemaVersion,
        data: envelope.event.toJson(),
      },
      metadata: metadataToJson(envelope.metadata),
    };
  }

  /** Deserialize an event envelope from JSON */
  static deserialize(json: JsonObject): EventEnvelope {
    const eventData = json.event as JsonObject;
    const metadata = metadataFromJson(json.metadata as JsonObject);

    const eventType = eventData.eventType as EventType;
    const EventClass = this.eventRegistry.get(eventType);

    if (!EventClass) {
      throw new Error(`Unknown event type: ${eventType}`);
    }

    // Create event instance and populate from data
    const event = new EventClass();
    Object.assign(event, eventData.data);

    return {
      event: event as DomainEvent,
      metadata,
    };
  }
}

/** Parameters required to construct metadata for an event */
export interface EventFactoryParams {
  aggregateId: string;
  aggregateType: string;
  aggregateVersion: Version;
  tenantId?: string;
  globalPosition?: number;
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
    aggregateVersion: Number(json.aggregateVersion ?? 0) as Version,
    aggregateId: String(json.aggregateId ?? ''),
    aggregateType: String(json.aggregateType ?? ''),
    tenantId: json.tenantId === undefined ? undefined : String(json.tenantId),
    globalPosition:
      json.globalPosition === undefined ? undefined : Number(json.globalPosition as number),
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
