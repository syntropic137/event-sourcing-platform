/**
 * Metadata utilities for event sourcing
 */

/** Metadata for aggregate classes */
export interface AggregateMetadata {
  readonly type: string;
  readonly version: number;
}

/** Store for aggregate metadata */
export class AggregateMetadataStore {
  private static metadata = new Map<Function, AggregateMetadata>();

  /** Register metadata for an aggregate class */
  static register(aggregateClass: Function, metadata: AggregateMetadata): void {
    this.metadata.set(aggregateClass, metadata);
  }

  /** Get metadata for an aggregate class */
  static get(aggregateClass: Function): AggregateMetadata | undefined {
    return this.metadata.get(aggregateClass);
  }
}

/** Decorator to mark an aggregate class */
export function AggregateRoot(type: string, version: number = 1) {
  return function <T extends Function>(constructor: T): T {
    AggregateMetadataStore.register(constructor, { type, version });
    return constructor;
  };
}
