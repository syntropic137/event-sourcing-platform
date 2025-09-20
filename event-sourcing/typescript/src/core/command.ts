/**
 * Command handling patterns and abstractions
 */

import { AggregateId } from '../types/common';
import { DomainEvent } from './event';
import { Aggregate } from './aggregate';

type AnyCommandHandler = CommandHandler<Command, DomainEvent>;

const COMMAND_HANDLER_MAP: unique symbol = Symbol('commandHandlerMap');

type CommandHandlerAwareConstructor = {
  [COMMAND_HANDLER_MAP]?: Map<string, string>;
};

function ensureCommandHandlerMap(ctor: CommandHandlerAwareConstructor): Map<string, string> {
  if (!ctor[COMMAND_HANDLER_MAP]) {
    ctor[COMMAND_HANDLER_MAP] = new Map<string, string>();
  }
  return ctor[COMMAND_HANDLER_MAP]!;
}

/** Base interface for commands */
export interface Command {
  /** The ID of the aggregate this command targets */
  readonly aggregateId: AggregateId;
}

/** Result of handling a command */
export interface CommandResult<TEvent extends DomainEvent = DomainEvent> {
  /** Events produced by handling the command */
  readonly events: TEvent[];

  /** Whether the command was successful */
  readonly success: boolean;

  /** Error message if the command failed */
  readonly error?: string;
}

/** Interface for command handlers */
export interface CommandHandler<
  TCommand extends Command,
  TEvent extends DomainEvent = DomainEvent,
> {
  /** Handle a command and return the result */
  handle(command: TCommand): Promise<CommandResult<TEvent>>;
}

/** Command handler that operates on an aggregate */
export interface AggregateCommandHandler<
  TAggregate extends Aggregate,
  TCommand extends Command,
  TEvent extends DomainEvent = DomainEvent,
> {
  /** Handle a command with access to the current aggregate state */
  handle(aggregate: TAggregate, command: TCommand): Promise<TEvent[]>;
}

/** Command bus for routing commands to handlers */
export interface CommandBus {
  /** Register a command handler */
  registerHandler<TCommand extends Command>(
    commandType: string,
    handler: CommandHandler<TCommand>
  ): void;

  /** Send a command and get the result */
  send<TCommand extends Command>(command: TCommand): Promise<CommandResult>;
}

/** Simple in-memory command bus implementation */
export class InMemoryCommandBus implements CommandBus {
  private handlers = new Map<string, AnyCommandHandler>();

  /** Register a command handler */
  registerHandler<TCommand extends Command>(
    commandType: string,
    handler: CommandHandler<TCommand>
  ): void {
    this.handlers.set(commandType, handler as AnyCommandHandler);
  }

  /** Send a command */
  async send<TCommand extends Command>(command: TCommand): Promise<CommandResult> {
    const commandType = command.constructor.name;
    const handler = this.handlers.get(commandType) as CommandHandler<TCommand> | undefined;

    if (!handler) {
      return {
        events: [],
        success: false,
        error: `No handler registered for command type: ${commandType}`,
      };
    }

    try {
      return await handler.handle(command);
    } catch (error) {
      return {
        events: [],
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error',
      };
    }
  }
}

/** Decorator for command handler methods */
// eslint-disable-next-line @typescript-eslint/no-redeclare
export function CommandHandler(commandType: string) {
  return function (target: object, propertyKey: string, descriptor: PropertyDescriptor) {
    // Store metadata for the command handler
    const ctor = target.constructor as CommandHandlerAwareConstructor;
    const handlers = ensureCommandHandlerMap(ctor);
    handlers.set(commandType, propertyKey);

    return descriptor;
  };
}

/** Base class for aggregate command handlers */
export abstract class BaseAggregateCommandHandler<
  TAggregate extends Aggregate,
  TCommand extends Command,
  TEvent extends DomainEvent = DomainEvent,
> implements AggregateCommandHandler<TAggregate, TCommand, TEvent>
{
  /** Handle the command - must be implemented by subclasses */
  abstract handle(aggregate: TAggregate, command: TCommand): Promise<TEvent[]>;

  /** Validate a command - can be overridden by subclasses */
  protected async validateCommand(command: TCommand): Promise<string[]> {
    const errors: string[] = [];

    // Basic validation
    if (!command.aggregateId) {
      errors.push('Aggregate ID is required');
    }

    return errors;
  }
}

/** Command validation result */
export interface ValidationResult {
  readonly isValid: boolean;
  readonly errors: string[];
}

/** Command validator interface */
export interface CommandValidator<TCommand extends Command> {
  /** Validate a command */
  validate(command: TCommand): Promise<ValidationResult>;
}
