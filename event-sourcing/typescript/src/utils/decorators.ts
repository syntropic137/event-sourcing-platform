/**
 * Utility decorators for event sourcing and CQRS
 */

// Re-export decorators from core modules for convenience
export { EventSourcingHandler } from '../core/aggregate';
export { CommandHandler } from '../core/command';
export { ProjectionHandler, QueryHandler } from '../core/query';
