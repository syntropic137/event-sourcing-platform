/**
 * Aggregate abstractions and base implementations for event sourcing
 */

import { AggregateId, Version } from '../types/common';
import { DomainEvent, EventEnvelope, EventFactory } from './event';
import { InvalidAggregateStateError } from './errors';

/** Core interface for event-sourced aggregates */
export interface Aggregate<TEvent extends DomainEvent = DomainEvent> {
  /** The aggregate's unique identifier */
  readonly id: AggregateId | null;

  /** The aggregate's current version for optimistic concurrency control */
  readonly version: Version;

  /** Apply an event to update the aggregate's state */
  applyEvent(event: TEvent): void;

  /** Get all uncommitted events */
  getUncommittedEvents(): EventEnvelope<TEvent>[];

  /** Mark all events as committed */
  markEventsAsCommitted(): void;

  /** Check if the aggregate has any uncommitted events */
  hasUncommittedEvents(): boolean;
}

/** Base class for event-sourced aggregates */
export abstract class BaseAggregate<TEvent extends DomainEvent = DomainEvent>
  implements Aggregate<TEvent>
{
  private _id: AggregateId | null = null;
  private _version: Version = 0;
  private _uncommittedEvents: EventEnvelope<TEvent>[] = [];

  /** Get the aggregate type name */
  abstract getAggregateType(): string;

  /** Apply an event to update the aggregate state - must be implemented by subclasses */
  abstract applyEvent(event: TEvent): void;

  /** Get the aggregate ID */
  get id(): AggregateId | null {
    return this._id;
  }

  /** Get the current version */
  get version(): Version {
    return this._version;
  }

  /** Get uncommitted events */
  getUncommittedEvents(): EventEnvelope<TEvent>[] {
    return [...this._uncommittedEvents];
  }

  /** Mark all events as committed */
  markEventsAsCommitted(): void {
    this._uncommittedEvents = [];
  }

  /** Check if there are uncommitted events */
  hasUncommittedEvents(): boolean {
    return this._uncommittedEvents.length > 0;
  }

  /** Set the aggregate ID (used during loading) */
  protected setId(id: AggregateId): void {
    if (this._id !== null && this._id !== id) {
      throw new InvalidAggregateStateError(
        this.getAggregateType(),
        `Cannot change aggregate ID from ${this._id} to ${id}`
      );
    }
    this._id = id;
  }

  /** Raise a new event */
  protected raiseEvent(event: TEvent): void {
    if (this._id === null) {
      throw new InvalidAggregateStateError(
        this.getAggregateType(),
        'Cannot raise events on an aggregate without an ID'
      );
    }

    // Create event envelope with metadata
    const envelope = EventFactory.create(event, {
      aggregateId: this._id,
      aggregateType: this.getAggregateType(),
      aggregateVersion: this._version + 1,
    });

    // Apply the event to update state
    this.applyEvent(event);

    // Increment version
    this._version++;

    // Add to uncommitted events
    this._uncommittedEvents.push(envelope);
  }

  /** Load the aggregate from a sequence of events */
  protected loadFromEvents(events: EventEnvelope<TEvent>[]): void {
    this._uncommittedEvents = [];

    for (const envelope of events) {
      // Set ID from first event
      if (this._id === null) {
        this.setId(envelope.metadata.aggregateId);
      }

      // Apply the event
      this.applyEvent(envelope.event);

      // Update version
      this._version = envelope.metadata.aggregateVersion;
    }
  }
}

/** Event sourcing handler decorator metadata */
export interface EventHandlerMetadata {
  eventType: string;
  methodName: string;
}

/** Decorator for marking event handler methods */
export function EventSourcingHandler(eventType: string) {
  return function (target: any, propertyKey: string, descriptor: PropertyDescriptor) {
    // Store metadata for the event handler
    const metadata: EventHandlerMetadata = {
      eventType,
      methodName: propertyKey,
    };

    // Store metadata on the class prototype
    if (!target.constructor._eventHandlers) {
      target.constructor._eventHandlers = new Map<string, string>();
    }
    target.constructor._eventHandlers.set(eventType, propertyKey);

    return descriptor;
  };
}

/** Advanced aggregate that supports automatic event dispatching */
export abstract class AutoDispatchAggregate<
  TEvent extends DomainEvent = DomainEvent,
> extends BaseAggregate<TEvent> {
  /** Apply event using automatic method dispatch */
  applyEvent(event: TEvent): void {
    const eventHandlers = (this.constructor as any)._eventHandlers as Map<string, string>;

    if (eventHandlers && eventHandlers.has(event.eventType)) {
      const methodName = eventHandlers.get(event.eventType)!;
      const handler = (this as any)[methodName];

      if (typeof handler === 'function') {
        handler.call(this, event);
        return;
      }
    }

    // Fallback to default handling
    this.handleUnknownEvent(event);
  }

  /** Handle unknown events - can be overridden by subclasses */
  protected handleUnknownEvent(event: TEvent): void {
    // Default: ignore unknown events
    console.warn(`No handler found for event type: ${event.eventType}`);
  }
}

/** Aggregate repository interface */
export interface AggregateRepository<TAggregate extends Aggregate> {
  /** Load an aggregate by ID */
  load(aggregateId: AggregateId): Promise<TAggregate | null>;

  /** Save an aggregate */
  save(aggregate: TAggregate): Promise<void>;

  /** Check if an aggregate exists */
  exists(aggregateId: AggregateId): Promise<boolean>;
}
