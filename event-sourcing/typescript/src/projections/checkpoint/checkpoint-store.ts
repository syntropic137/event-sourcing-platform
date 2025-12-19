/**
 * Checkpoint store interface and implementations
 */

import { ProjectionCheckpoint } from '../types';

/**
 * Interface for checkpoint persistence
 *
 * Checkpoints track which events have been processed by each projection,
 * enabling reliable resumption after restarts.
 */
export interface ProjectionCheckpointStore {
  /**
   * Get the checkpoint for a projection
   * @returns The checkpoint or null if not found
   */
  getCheckpoint(projectionName: string): Promise<ProjectionCheckpoint | null>;

  /**
   * Save a checkpoint
   */
  saveCheckpoint(checkpoint: ProjectionCheckpoint): Promise<void>;

  /**
   * Delete a checkpoint (for rebuild)
   */
  deleteCheckpoint(projectionName: string): Promise<void>;

  /**
   * Get all checkpoints
   */
  getAllCheckpoints(): Promise<ProjectionCheckpoint[]>;

  /**
   * Get the minimum position across all tracked projections
   * Used to determine where to start reading from event store
   */
  getMinimumPosition(): Promise<number>;
}

/**
 * In-memory checkpoint store for testing
 */
export class MemoryCheckpointStore implements ProjectionCheckpointStore {
  private checkpoints = new Map<string, ProjectionCheckpoint>();

  async getCheckpoint(projectionName: string): Promise<ProjectionCheckpoint | null> {
    return this.checkpoints.get(projectionName) ?? null;
  }

  async saveCheckpoint(checkpoint: ProjectionCheckpoint): Promise<void> {
    this.checkpoints.set(checkpoint.projectionName, { ...checkpoint });
  }

  async deleteCheckpoint(projectionName: string): Promise<void> {
    this.checkpoints.delete(projectionName);
  }

  async getAllCheckpoints(): Promise<ProjectionCheckpoint[]> {
    return Array.from(this.checkpoints.values());
  }

  async getMinimumPosition(): Promise<number> {
    const checkpoints = await this.getAllCheckpoints();
    if (checkpoints.length === 0) {
      return 0;
    }
    return Math.min(...checkpoints.map((c) => c.globalPosition));
  }

  /**
   * Clear all checkpoints (for testing)
   */
  clear(): void {
    this.checkpoints.clear();
  }
}
