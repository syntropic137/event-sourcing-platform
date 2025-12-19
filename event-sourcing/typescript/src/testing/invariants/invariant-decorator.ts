/**
 * @Invariant decorator for defining aggregate business rules
 *
 * Invariants are conditions that must always be true for an aggregate.
 * They are checked after each event is applied during testing.
 */

/**
 * Metadata stored for each invariant
 */
export interface InvariantMetadata {
  /** Human-readable description of the invariant */
  description: string;

  /** Method name that checks the invariant */
  methodName: string;

  /** Optional severity (error = test fails, warning = logged but passes) */
  severity: 'error' | 'warning';
}

/**
 * Symbol for storing invariant metadata on aggregate classes
 */
export const INVARIANT_METADATA: unique symbol = Symbol('invariantMetadata');

/**
 * Type for aggregate classes that have invariant metadata
 */
export type InvariantAwareConstructor = {
  [INVARIANT_METADATA]?: Map<string, InvariantMetadata>;
};

/**
 * Ensure invariant map exists on constructor
 */
function ensureInvariantMap(ctor: InvariantAwareConstructor): Map<string, InvariantMetadata> {
  if (!ctor[INVARIANT_METADATA]) {
    ctor[INVARIANT_METADATA] = new Map<string, InvariantMetadata>();
  }
  return ctor[INVARIANT_METADATA]!;
}

/**
 * Options for the @Invariant decorator
 */
export interface InvariantOptions {
  /** Severity level - 'error' (default) fails tests, 'warning' only logs */
  severity?: 'error' | 'warning';
}

/**
 * Decorator for marking methods as aggregate invariants
 *
 * Invariant methods must:
 * - Return a boolean (true = invariant holds, false = violated)
 * - Take no arguments
 * - Not modify aggregate state
 *
 * @param description - Human-readable description of the invariant
 * @param options - Optional configuration
 *
 * @example
 * ```typescript
 * @Aggregate('BankAccount')
 * class BankAccountAggregate extends AggregateRoot<BankAccountEvent> {
 *   private balance: number = 0;
 *
 *   @Invariant('balance must never be negative')
 *   private checkBalance(): boolean {
 *     return this.balance >= 0;
 *   }
 *
 *   @Invariant('closed accounts cannot have pending transactions', { severity: 'warning' })
 *   private checkClosedState(): boolean {
 *     return !(this.isClosed && this.pendingTransactions.length > 0);
 *   }
 * }
 * ```
 */
export function Invariant(description: string, options: InvariantOptions = {}) {
  return function (
    target: object,
    propertyKey: string,
    descriptor: PropertyDescriptor
  ): PropertyDescriptor {
    const metadata: InvariantMetadata = {
      description,
      methodName: propertyKey,
      severity: options.severity ?? 'error',
    };

    const ctor = target.constructor as InvariantAwareConstructor;
    const invariants = ensureInvariantMap(ctor);
    invariants.set(propertyKey, metadata);

    return descriptor;
  };
}

/**
 * Get all invariants defined on an aggregate class
 */
export function getInvariants(
  aggregateClass: InvariantAwareConstructor
): Map<string, InvariantMetadata> {
  return aggregateClass[INVARIANT_METADATA] ?? new Map();
}

/**
 * Check if a class has any invariants defined
 */
export function hasInvariants(aggregateClass: InvariantAwareConstructor): boolean {
  const invariants = aggregateClass[INVARIANT_METADATA];
  return invariants !== undefined && invariants.size > 0;
}
