/**
 * Metadata utilities for event sourcing
 */

/** Metadata for aggregate classes */
export interface AggregateMetadata {
  readonly type: string;
  readonly version: number;
}

type AggregateConstructor<T = unknown> = abstract new (...args: unknown[]) => T;

/** Store for aggregate metadata */
export class AggregateMetadataStore {
  private static readonly metadata = new Map<AggregateConstructor, AggregateMetadata>();

  /** Register metadata for an aggregate class */
  static register<T>(aggregateClass: AggregateConstructor<T>, metadata: AggregateMetadata): void {
    this.metadata.set(aggregateClass, metadata);
  }

  /** Get metadata for an aggregate class */
  static get<T>(aggregateClass: AggregateConstructor<T>): AggregateMetadata | undefined {
    return this.metadata.get(aggregateClass);
  }
}

/** Decorator to mark an aggregate class */
export function AggregateRoot(type: string, version: number = 1) {
  return function <T extends AggregateConstructor>(constructor: T): T {
    AggregateMetadataStore.register(constructor, { type, version });
    return constructor;
  };
}
