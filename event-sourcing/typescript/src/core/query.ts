/**
 * Query handling and projection building for CQRS read models
 */

import { DomainEvent, EventEnvelope } from './event';

/** Base interface for queries */
export interface Query {
  /** Query identifier for routing */
  readonly queryId?: string;
}

/** Result of executing a query */
export interface QueryResult<TData = unknown> {
  /** The query result data */
  readonly data: TData;

  /** Whether the query was successful */
  readonly success: boolean;

  /** Error message if the query failed */
  readonly error?: string;
}

/** Interface for query handlers */
export interface QueryHandler<TQuery extends Query, TResult = unknown> {
  /** Handle a query and return the result */
  handle(query: TQuery): Promise<QueryResult<TResult>>;

  /** Check if this handler can handle the given query type */
  canHandle(queryType: string): boolean;
}

/** Query bus for routing queries to handlers */
export interface QueryBus {
  /** Register a query handler */
  registerHandler<TQuery extends Query>(queryType: string, handler: QueryHandler<TQuery>): void;

  /** Send a query and get the result */
  send<TQuery extends Query, TResult = unknown>(query: TQuery): Promise<QueryResult<TResult>>;
}

/** Simple in-memory query bus implementation */
export class InMemoryQueryBus implements QueryBus {
  private handlers = new Map<string, QueryHandler<any>>();

  /** Register a query handler */
  registerHandler<TQuery extends Query>(queryType: string, handler: QueryHandler<TQuery>): void {
    this.handlers.set(queryType, handler);
  }

  /** Send a query */
  async send<TQuery extends Query, TResult = unknown>(
    query: TQuery
  ): Promise<QueryResult<TResult>> {
    const queryType = query.constructor.name;
    const handler = this.handlers.get(queryType);

    if (!handler) {
      return {
        data: null as TResult,
        success: false,
        error: `No handler registered for query type: ${queryType}`,
      };
    }

    try {
      return (await handler.handle(query)) as QueryResult<TResult>;
    } catch (error) {
      return {
        data: null as TResult,
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error',
      };
    }
  }
}

/** Decorator for query handler methods */
export function QueryHandler(queryType: string) {
  return function (target: any, propertyKey: string, descriptor: PropertyDescriptor) {
    // Store metadata for the query handler
    if (!target.constructor._queryHandlers) {
      target.constructor._queryHandlers = new Map<string, string>();
    }
    target.constructor._queryHandlers.set(queryType, propertyKey);

    return descriptor;
  };
}

/** Interface for event projections that build read models */
export interface Projection<TEvent extends DomainEvent = DomainEvent> {
  /** Handle an event and update the projection */
  handleEvent(event: EventEnvelope<TEvent>): Promise<void>;

  /** Get the projection name */
  getName(): string;

  /** Get the projection version for tracking schema changes */
  getVersion(): number;
}

/** Projection manager that coordinates multiple projections */
export interface ProjectionManager {
  /** Register a projection */
  register<TEvent extends DomainEvent>(projection: Projection<TEvent>): void;

  /** Process an event through all relevant projections */
  processEvent<TEvent extends DomainEvent>(event: EventEnvelope<TEvent>): Promise<void>;

  /** Start processing events */
  start(): Promise<void>;

  /** Stop processing events */
  stop(): Promise<void>;
}

/** Base class for projections */
export abstract class BaseProjection<TEvent extends DomainEvent = DomainEvent>
  implements Projection<TEvent>
{
  abstract getName(): string;
  abstract getVersion(): number;
  abstract handleEvent(event: EventEnvelope<TEvent>): Promise<void>;

  /** Check if this projection handles a specific event type */
  protected handlesEventType(eventType: string): boolean {
    // Default implementation - subclasses should override
    return true;
  }
}

/** Simple in-memory projection manager */
export class InMemoryProjectionManager implements ProjectionManager {
  private projections: Projection[] = [];
  private isRunning = false;

  /** Register a projection */
  register<TEvent extends DomainEvent>(projection: Projection<TEvent>): void {
    this.projections.push(projection);
  }

  /** Process an event through all projections */
  async processEvent<TEvent extends DomainEvent>(event: EventEnvelope<TEvent>): Promise<void> {
    if (!this.isRunning) {
      throw new Error('Projection manager is not running');
    }

    // Process event through all projections in parallel
    const promises = this.projections.map(async (projection) => {
      try {
        await projection.handleEvent(event);
      } catch (error) {
        console.error(`Error in projection ${projection.getName()}:`, error);
        // TODO: Add proper error handling and retry logic
      }
    });

    await Promise.all(promises);
  }

  /** Start the projection manager */
  async start(): Promise<void> {
    this.isRunning = true;
    console.log(`Started projection manager with ${this.projections.length} projections`);
  }

  /** Stop the projection manager */
  async stop(): Promise<void> {
    this.isRunning = false;
    console.log('Stopped projection manager');
  }
}

/** Event handler decorator for projection methods */
export function ProjectionHandler(eventType: string) {
  return function (target: any, propertyKey: string, descriptor: PropertyDescriptor) {
    // Store metadata for the event handler
    if (!target.constructor._projectionHandlers) {
      target.constructor._projectionHandlers = new Map<string, string>();
    }
    target.constructor._projectionHandlers.set(eventType, propertyKey);

    return descriptor;
  };
}

/** Auto-dispatch projection that routes events to handler methods */
export abstract class AutoDispatchProjection<
  TEvent extends DomainEvent = DomainEvent,
> extends BaseProjection<TEvent> {
  /** Handle event using automatic method dispatch */
  async handleEvent(event: EventEnvelope<TEvent>): Promise<void> {
    const handlers = (this.constructor as any)._projectionHandlers as Map<string, string>;

    if (handlers && handlers.has(event.event.eventType)) {
      const methodName = handlers.get(event.event.eventType)!;
      const handler = (this as any)[methodName];

      if (typeof handler === 'function') {
        await handler.call(this, event);
        return;
      }
    }

    // Fallback to default handling
    await this.handleUnknownEvent(event);
  }

  /** Handle unknown events - can be overridden by subclasses */
  protected async handleUnknownEvent(event: EventEnvelope<TEvent>): Promise<void> {
    // Default: ignore unknown events
    console.debug(
      `No handler found for event type: ${event.event.eventType} in projection ${this.getName()}`
    );
  }
}

/** Projection state tracking */
export interface ProjectionState {
  readonly projectionName: string;
  readonly version: number;
  readonly lastProcessedEventId: string;
  readonly lastProcessedTimestamp: string;
  readonly isActive: boolean;
}

/** Projection state store interface */
export interface ProjectionStateStore {
  /** Get projection state */
  getState(projectionName: string): Promise<ProjectionState | null>;

  /** Update projection state */
  updateState(state: ProjectionState): Promise<void>;

  /** Get all projection states */
  getAllStates(): Promise<ProjectionState[]>;
}
