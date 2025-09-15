/**
 * Event definitions and metadata handling for the event sourcing SDK
 */

import { UUID, Timestamp, JsonObject, JsonValue, EventType, Version } from '../types/common';

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

  /** Version of the aggregate when this event was created */
  readonly aggregateVersion: Version;

  /** ID of the aggregate that produced this event */
  readonly aggregateId: string;

  /** Type of the aggregate that produced this event */
  readonly aggregateType: string;

  /** Additional metadata */
  readonly metadata: Record<string, unknown>;
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
  static createMetadata(params: {
    aggregateId: string;
    aggregateType: string;
    aggregateVersion: Version;
    metadata?: Record<string, unknown>;
  }): EventMetadata {
    return {
      eventId: crypto.randomUUID(),
      timestamp: new Date().toISOString(),
      aggregateVersion: params.aggregateVersion,
      aggregateId: params.aggregateId,
      aggregateType: params.aggregateType,
      metadata: params.metadata ?? {},
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
    params: {
      aggregateId: string;
      aggregateType: string;
      aggregateVersion: Version;
      metadata?: Record<string, unknown>;
    }
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
      metadata: envelope.metadata as unknown as Record<string, JsonValue>,
    };
  }

  /** Deserialize an event envelope from JSON */
  static deserialize(json: JsonObject): EventEnvelope {
    const eventData = json.event as JsonObject;
    const metadata = json.metadata as unknown as EventMetadata;

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
