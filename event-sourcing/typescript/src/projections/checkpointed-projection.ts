/**
 * CheckpointedProjection - Base class for projections with mandatory checkpoint tracking
 *
 * All projections should extend this class to ensure reliable event processing
 * with proper checkpoint management.
 */

import { DomainEvent, EventEnvelope } from '../core/event';
import { ProjectionResult } from './types';
import { ProjectionCheckpointStore } from './checkpoint/checkpoint-store';

/**
 * Base class for projections with mandatory checkpoint tracking
 *
 * Subclasses must implement:
 * - `getName()` - Unique projection identifier
 * - `getVersion()` - Schema version (increment to trigger rebuild)
 * - `getSubscribedEventTypes()` - Event types to process (or null for all)
 * - `processEvent()` - Event handling logic
 *
 * @example
 * ```typescript
 * class OrderSummaryProjection extends CheckpointedProjection {
 *   getName(): string { return 'order-summary'; }
 *   getVersion(): number { return 1; }
 *   getSubscribedEventTypes(): Set<string> { return new Set(['OrderCreated', 'OrderShipped']); }
 *
 *   protected async processEvent(
 *     envelope: EventEnvelope,
 *     checkpointStore: ProjectionCheckpointStore
 *   ): Promise<ProjectionResult> {
 *     // Handle event...
 *     await this.saveCheckpoint(checkpointStore, envelope.metadata.globalNonce!);
 *     return ProjectionResult.SUCCESS;
 *   }
 * }
 * ```
 */
export abstract class CheckpointedProjection<TEvent extends DomainEvent = DomainEvent> {
  /**
   * Get the unique projection name
   * Used for checkpoint tracking and identification
   */
  abstract getName(): string;

  /**
   * Get the projection schema version
   * Increment this when the projection logic changes and requires a rebuild
   */
  abstract getVersion(): number;

  /**
   * Get the event types this projection subscribes to
   * Return null to receive all events
   */
  abstract getSubscribedEventTypes(): Set<string> | null;

  /**
   * Process an event and return the result
   *
   * Implementations MUST:
   * - Save checkpoint on success (call `saveCheckpoint`)
   * - Return appropriate ProjectionResult
   * - Handle errors and return RETRY or FAILURE
   *
   * @param envelope - The event to process
   * @param checkpointStore - Store for saving checkpoints
   * @returns Processing result
   */
  protected abstract processEvent(
    envelope: EventEnvelope<TEvent>,
    checkpointStore: ProjectionCheckpointStore
  ): Promise<ProjectionResult>;

  /**
   * Handle an event with checkpoint tracking
   * This is the entry point called by the subscription coordinator
   */
  async handleEvent(
    envelope: EventEnvelope<TEvent>,
    checkpointStore: ProjectionCheckpointStore
  ): Promise<ProjectionResult> {
    const subscribedTypes = this.getSubscribedEventTypes();
    const eventType = envelope.event.eventType;

    // Skip if not subscribed to this event type
    if (subscribedTypes !== null && !subscribedTypes.has(eventType)) {
      // Still advance checkpoint for skipped events
      await this.saveCheckpoint(checkpointStore, envelope.metadata.globalNonce!);
      return ProjectionResult.SKIP;
    }

    // Check if we've already processed this event
    const checkpoint = await checkpointStore.getCheckpoint(this.getName());
    const globalNonce = envelope.metadata.globalNonce ?? 0;

    if (checkpoint && checkpoint.globalPosition >= globalNonce) {
      // Already processed
      return ProjectionResult.SKIP;
    }

    // Process the event
    return this.processEvent(envelope, checkpointStore);
  }

  /**
   * Save a checkpoint after successful processing
   *
   * Call this from `processEvent` after successfully handling an event
   */
  protected async saveCheckpoint(
    checkpointStore: ProjectionCheckpointStore,
    globalPosition: number
  ): Promise<void> {
    await checkpointStore.saveCheckpoint({
      projectionName: this.getName(),
      globalPosition,
      updatedAt: new Date(),
      version: this.getVersion(),
    });
  }

  /**
   * Clear all projection data for rebuild
   * Override this if the projection has persistent storage
   */
  async clearData(): Promise<void> {
    // Default implementation does nothing
    // Override to clear projection-specific storage
  }

  /**
   * Called before starting to process events
   * Override to perform initialization
   */
  async initialize(): Promise<void> {
    // Default implementation does nothing
  }

  /**
   * Called when the projection is shutting down
   * Override to perform cleanup
   */
  async shutdown(): Promise<void> {
    // Default implementation does nothing
  }
}

/**
 * Simple in-memory projection for testing and examples
 */
export abstract class InMemoryProjection<
  TState,
  TEvent extends DomainEvent = DomainEvent,
> extends CheckpointedProjection<TEvent> {
  protected state: TState;
  private readonly initialState: TState;

  constructor(initialState: TState) {
    super();
    this.initialState = initialState;
    this.state = this.cloneState(initialState);
  }

  /**
   * Get the current projection state
   */
  getState(): TState {
    return this.state;
  }

  /**
   * Clear the projection state (for rebuild)
   */
  async clearData(): Promise<void> {
    this.state = this.cloneState(this.initialState);
  }

  /**
   * Clone state (override for custom cloning)
   */
  protected cloneState(state: TState): TState {
    return JSON.parse(JSON.stringify(state));
  }
}
