/**
 * Failed event store (Dead Letter Queue) interface and implementations
 */

import { FailedEvent, FailedEventStatus } from '../types';

/**
 * Interface for storing failed events (Dead Letter Queue)
 */
export interface FailedEventStore {
  /**
   * Save a failed event
   */
  save(event: FailedEvent): Promise<void>;

  /**
   * Get failed events for a specific projection
   */
  getByProjection(
    projectionName: string,
    options?: { status?: FailedEventStatus; limit?: number }
  ): Promise<FailedEvent[]>;

  /**
   * Get failed events by status
   */
  getByStatus(status: FailedEventStatus, limit?: number): Promise<FailedEvent[]>;

  /**
   * Get a specific failed event by ID
   */
  getById(id: string): Promise<FailedEvent | null>;

  /**
   * Mark an event as being reprocessed
   */
  markReprocessing(id: string): Promise<void>;

  /**
   * Mark an event as resolved (successfully reprocessed)
   */
  markResolved(id: string): Promise<void>;

  /**
   * Mark an event as ignored (won't be reprocessed)
   */
  markIgnored(id: string, reason: string): Promise<void>;

  /**
   * Get count of failed events by projection
   */
  getCountByProjection(projectionName: string): Promise<number>;

  /**
   * Get total count of pending failed events
   */
  getPendingCount(): Promise<number>;

  /**
   * Delete resolved events older than a given date
   */
  cleanupResolved(olderThan: Date): Promise<number>;
}

/**
 * In-memory failed event store for testing
 */
export class MemoryFailedEventStore implements FailedEventStore {
  private events = new Map<string, FailedEvent>();

  async save(event: FailedEvent): Promise<void> {
    this.events.set(event.id, { ...event });
  }

  async getByProjection(
    projectionName: string,
    options?: { status?: FailedEventStatus; limit?: number }
  ): Promise<FailedEvent[]> {
    let events = Array.from(this.events.values()).filter(
      (e) => e.projectionName === projectionName
    );

    if (options?.status) {
      events = events.filter((e) => e.status === options.status);
    }

    events.sort((a, b) => b.lastFailedAt.getTime() - a.lastFailedAt.getTime());

    if (options?.limit) {
      events = events.slice(0, options.limit);
    }

    return events;
  }

  async getByStatus(status: FailedEventStatus, limit?: number): Promise<FailedEvent[]> {
    let events = Array.from(this.events.values()).filter((e) => e.status === status);

    events.sort((a, b) => b.lastFailedAt.getTime() - a.lastFailedAt.getTime());

    if (limit) {
      events = events.slice(0, limit);
    }

    return events;
  }

  async getById(id: string): Promise<FailedEvent | null> {
    return this.events.get(id) ?? null;
  }

  async markReprocessing(id: string): Promise<void> {
    const event = this.events.get(id);
    if (event) {
      event.status = 'reprocessing';
    }
  }

  async markResolved(id: string): Promise<void> {
    const event = this.events.get(id);
    if (event) {
      event.status = 'resolved';
    }
  }

  async markIgnored(id: string, reason: string): Promise<void> {
    const event = this.events.get(id);
    if (event) {
      event.status = 'ignored';
      event.ignoreReason = reason;
    }
  }

  async getCountByProjection(projectionName: string): Promise<number> {
    return Array.from(this.events.values()).filter(
      (e) => e.projectionName === projectionName && e.status === 'pending'
    ).length;
  }

  async getPendingCount(): Promise<number> {
    return Array.from(this.events.values()).filter((e) => e.status === 'pending').length;
  }

  async cleanupResolved(olderThan: Date): Promise<number> {
    let count = 0;
    for (const [id, event] of this.events) {
      if (event.status === 'resolved' && event.lastFailedAt < olderThan) {
        this.events.delete(id);
        count++;
      }
    }
    return count;
  }

  /**
   * Clear all events (for testing)
   */
  clear(): void {
    this.events.clear();
  }

  /**
   * Get all events (for testing)
   */
  getAll(): FailedEvent[] {
    return Array.from(this.events.values());
  }
}

/**
 * Generate a unique ID for a failed event
 */
export function generateFailedEventId(): string {
  return `failed-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
}
