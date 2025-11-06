/**
 * Query handling and projection building for CQRS read models
 */

import { DomainEvent, EventEnvelope } from './event';

type AnyQueryHandler = QueryHandler<Query, unknown>;

const QUERY_HANDLER_MAP: unique symbol = Symbol('queryHandlerMap');
const PROJECTION_HANDLER_MAP: unique symbol = Symbol('projectionHandlerMap');

type QueryHandlerAwareConstructor = {
  [QUERY_HANDLER_MAP]?: Map<string, string>;
};

type ProjectionHandlerAwareConstructor = {
  [PROJECTION_HANDLER_MAP]?: Map<string, string>;
};

function ensureQueryHandlerMap(ctor: QueryHandlerAwareConstructor): Map<string, string> {
  if (!ctor[QUERY_HANDLER_MAP]) {
    ctor[QUERY_HANDLER_MAP] = new Map<string, string>();
  }
  return ctor[QUERY_HANDLER_MAP]!;
}

function ensureProjectionHandlerMap(ctor: ProjectionHandlerAwareConstructor): Map<string, string> {
  if (!ctor[PROJECTION_HANDLER_MAP]) {
    ctor[PROJECTION_HANDLER_MAP] = new Map<string, string>();
  }
  return ctor[PROJECTION_HANDLER_MAP]!;
}

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
  private handlers = new Map<string, AnyQueryHandler>();

  /** Register a query handler */
  registerHandler<TQuery extends Query>(queryType: string, handler: QueryHandler<TQuery>): void {
    this.handlers.set(queryType, handler as AnyQueryHandler);
  }

  /** Send a query */
  async send<TQuery extends Query, TResult = unknown>(
    query: TQuery
  ): Promise<QueryResult<TResult>> {
    const queryType = query.constructor.name;
    const handler = this.handlers.get(queryType) as QueryHandler<TQuery, TResult> | undefined;

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
// eslint-disable-next-line @typescript-eslint/no-redeclare
export function QueryHandler(queryType: string) {
  return function (target: object, propertyKey: string, descriptor: PropertyDescriptor) {
    // Store metadata for the query handler
    const ctor = target.constructor as QueryHandlerAwareConstructor;
    const handlers = ensureQueryHandlerMap(ctor);
    handlers.set(queryType, propertyKey);

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
  protected handlesEventType(_eventType: string): boolean {
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
  return function (target: object, propertyKey: string, descriptor: PropertyDescriptor) {
    // Store metadata for the event handler
    const ctor = target.constructor as ProjectionHandlerAwareConstructor;
    const handlers = ensureProjectionHandlerMap(ctor);
    handlers.set(eventType, propertyKey);

    return descriptor;
  };
}

/** Auto-dispatch projection that routes events to handler methods */
export abstract class AutoDispatchProjection<
  TEvent extends DomainEvent = DomainEvent,
> extends BaseProjection<TEvent> {
  /** Handle event using automatic method dispatch */
  async handleEvent(event: EventEnvelope<TEvent>): Promise<void> {
    const ctor = this.constructor as ProjectionHandlerAwareConstructor;
    const handlers = ctor[PROJECTION_HANDLER_MAP];

    if (handlers && handlers.has(event.event.eventType)) {
      const methodName = handlers.get(event.event.eventType)!;
      const handler = (this as Record<string, unknown>)[methodName];

      if (typeof handler === 'function') {
        await (handler as (payload: EventEnvelope<TEvent>) => Promise<void> | void).call(
          this,
          event
        );
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

// ============================================================================
// QUERY DECORATOR (ADR-010)
// ============================================================================

/** Query metadata storage symbol */
export const QUERY_METADATA: unique symbol = Symbol('queryMetadata');

/** Query metadata */
export interface QueryDecoratorMetadata {
  queryType: string;
  description?: string;
}

/** Type-aware constructor with query metadata */
export type QueryAwareConstructor = {
  [QUERY_METADATA]?: QueryDecoratorMetadata;
};

/**
 * Decorator for query classes to store metadata about query type.
 * This enables the VSA CLI to discover and validate queries automatically.
 *
 * @param queryType - The query type identifier (e.g., "GetTaskById")
 * @param description - Optional description of what the query does
 *
 * @example
 * ```typescript
 * @Query("GetTaskById", "Retrieves a task by its ID")
 * export class GetTaskByIdQuery implements Query {
 *   constructor(public readonly taskId: string) {}
 * }
 * ```
 *
 * @see ADR-006: Domain Organization Pattern
 * @see ADR-009: CQRS Pattern Implementation
 * @see ADR-010: Decorator Patterns for Framework Integration
 */
// eslint-disable-next-line @typescript-eslint/no-redeclare
export function Query(queryType: string, description?: string) {
  return function <T extends new (...args: any[]) => any>(constructor: T): T {
    // Store metadata on the constructor
    (constructor as QueryAwareConstructor)[QUERY_METADATA] = {
      queryType,
      description,
    };

    return constructor;
  };
}

/**
 * Get query metadata from a query class
 */
export function getQueryMetadata(
  queryClass: QueryAwareConstructor
): QueryDecoratorMetadata | undefined {
  return queryClass[QUERY_METADATA];
}
