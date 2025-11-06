/**
 * Aggregate abstractions and base implementations for event sourcing
 */

import { AggregateId, Version } from '../types/common';
import { DomainEvent, EventEnvelope, EventFactory } from './event';
import { InvalidAggregateStateError } from './errors';
import { COMMAND_HANDLER_MAP, CommandHandlerAwareConstructor } from './command';

const EVENT_HANDLER_MAP: unique symbol = Symbol('eventHandlerMap');

type EventHandlerAwareConstructor = {
  [EVENT_HANDLER_MAP]?: Map<string, EventHandlerMetadata>;
};

function ensureEventHandlerMap(
  ctor: EventHandlerAwareConstructor
): Map<string, EventHandlerMetadata> {
  if (!ctor[EVENT_HANDLER_MAP]) {
    ctor[EVENT_HANDLER_MAP] = new Map<string, EventHandlerMetadata>();
  }
  return ctor[EVENT_HANDLER_MAP]!;
}

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

  /** Rehydrate the aggregate from committed history */
  rehydrate(events: EventEnvelope<TEvent>[]): void;
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

  /** Initialize a brand-new aggregate instance with an identifier */
  protected initialize(id: AggregateId): void {
    this.setId(id);
    this._version = 0;
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
      aggregateNonce: this._version + 1,
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
    this._id = null;
    this._version = 0;

    for (const envelope of events) {
      if (this._id === null) {
        this.setId(envelope.metadata.aggregateId);
      }

      this.applyEvent(envelope.event);
      this._version = envelope.metadata.aggregateNonce;
    }
  }

  /** Public rehydration entry point used by repositories */
  public rehydrate(events: EventEnvelope<TEvent>[]): void {
    this.loadFromEvents(events);
  }
}

/** Event sourcing handler decorator metadata */
export interface EventHandlerMetadata {
  eventType: string;
  methodName: string;
}

/** Aggregate decorator metadata */
const AGGREGATE_METADATA: unique symbol = Symbol('aggregateMetadata');

interface AggregateMetadata {
  aggregateType?: string;
}

type AggregateAwareConstructor = {
  [AGGREGATE_METADATA]?: AggregateMetadata;
};

/** Decorator for marking aggregate root classes */
export function AggregateDecorator(aggregateType?: string) {
  return function <T extends new (...args: unknown[]) => object>(constructor: T) {
    const metadata: AggregateMetadata = {
      aggregateType: aggregateType || constructor.name,
    };

    (constructor as AggregateAwareConstructor)[AGGREGATE_METADATA] = metadata;
    return constructor;
  };
}

/** Decorator for marking event handler methods */
export function EventSourcingHandler(eventType: string) {
  return function (target: object, propertyKey: string, descriptor: PropertyDescriptor) {
    // Store metadata for the event handler
    const metadata: EventHandlerMetadata = {
      eventType,
      methodName: propertyKey,
    };

    const ctor = target.constructor as EventHandlerAwareConstructor;
    const handlers = ensureEventHandlerMap(ctor);
    handlers.set(eventType, metadata);

    return descriptor;
  };
}

/**
 * AggregateRoot - Production-ready aggregate base class
 * This is the main class that aggregates should extend
 *
 * Features:
 * - Automatic event dispatching via @EventSourcingHandler decorators
 * - Command handling via @CommandHandler decorators
 * - Full event sourcing lifecycle support
 */
export abstract class AggregateRoot<
  TEvent extends DomainEvent = DomainEvent,
> extends BaseAggregate<TEvent> {
  /** Get the aggregate identifier */
  get aggregateId(): AggregateId | null {
    return this.id;
  }

  /**
   * Apply event using automatic method dispatch
   * Automatically routes events to methods decorated with @EventSourcingHandler
   */
  applyEvent(event: TEvent): void {
    const ctor = this.constructor as EventHandlerAwareConstructor;
    const eventHandlers = ctor[EVENT_HANDLER_MAP];

    if (eventHandlers && eventHandlers.has(event.eventType)) {
      const handlerMetadata = eventHandlers.get(event.eventType)!;
      const handler = (this as Record<string, unknown>)[handlerMetadata.methodName];

      if (typeof handler === 'function') {
        (handler as (payload: TEvent) => void).call(this, event);
        return;
      }
    }

    // Fallback to default handling
    this.handleUnknownEvent(event);
  }

  /**
   * Handle unknown events - can be overridden by subclasses
   * Default behavior: log a warning and ignore the event
   */
  protected handleUnknownEvent(event: TEvent): void {
    console.warn(`No handler found for event type: ${event.eventType}`);
  }

  /**
   * Apply an event - this is the main method for emitting events from command handlers
   * This method both applies the event to the aggregate state and adds it to uncommitted events
   */
  protected apply(event: TEvent): void {
    this.raiseEvent(event);
  }

  /**
   * Initialize the aggregate with an ID - used when creating new aggregates
   */
  protected initialize(aggregateId: AggregateId): void {
    super.initialize(aggregateId);
  }

  /**
   * Get the aggregate type from decorator metadata or class name
   */
  getAggregateType(): string {
    const ctor = this.constructor as AggregateAwareConstructor;
    const metadata = ctor[AGGREGATE_METADATA];
    return metadata?.aggregateType || this.constructor.name;
  }

  /**
   * Handle a command by dispatching to the appropriate @CommandHandler method
   * This method reads the @CommandHandler decorator metadata and invokes the decorated method
   */
  protected handleCommand<TCommand extends object>(command: TCommand): void {
    const commandType = (command as { constructor: { name: string } }).constructor.name;
    const ctor = this.constructor as unknown as CommandHandlerAwareConstructor;
    const handlers = ctor[COMMAND_HANDLER_MAP];

    if (!handlers || !handlers.has(commandType)) {
      throw new Error(
        `No @CommandHandler found for command type: ${commandType} on aggregate ${this.getAggregateType()}`
      );
    }

    const methodName = handlers.get(commandType)!;
    const handler = (this as Record<string, unknown>)[methodName];

    if (typeof handler !== 'function') {
      throw new Error(
        `Command handler method '${methodName}' is not a function on aggregate ${this.getAggregateType()}`
      );
    }

    // Invoke the command handler method
    (handler as (command: TCommand) => void).call(this, command);
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
