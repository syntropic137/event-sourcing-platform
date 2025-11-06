/**
 * Minimal type stubs for @event-sourcing-platform/core
 * These fixtures are for VSA CLI scanning tests, not execution
 */

declare module '@event-sourcing-platform/core' {
  // Decorator types
  export function Event(eventType: string, version: string): ClassDecorator;
  export function Command(commandType: string, description?: string): ClassDecorator;
  export function Query(queryType: string, description?: string): ClassDecorator;
  export function Aggregate(name: string): ClassDecorator;
}


