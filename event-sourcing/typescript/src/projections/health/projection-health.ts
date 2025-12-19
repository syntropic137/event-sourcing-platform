/**
 * Projection health checking utilities
 */

import { ProjectionHealth } from '../types';
import { ProjectionCheckpointStore } from '../checkpoint/checkpoint-store';
import { FailedEventStore } from '../failure/failed-event-store';

/**
 * Configuration for health thresholds
 */
export interface HealthThresholds {
  /** Lag threshold for degraded status */
  degradedLagThreshold: number;

  /** Lag threshold for unhealthy status */
  unhealthyLagThreshold: number;

  /** Maximum age of last processed event for healthy status (ms) */
  maxHealthyAgeMs: number;
}

/**
 * Default health thresholds
 */
export const DEFAULT_HEALTH_THRESHOLDS: HealthThresholds = {
  degradedLagThreshold: 100,
  unhealthyLagThreshold: 1000,
  maxHealthyAgeMs: 60000, // 1 minute
};

/**
 * Interface for getting current event store position
 */
export interface PositionProvider {
  getCurrentPosition(): Promise<number>;
}

/**
 * Configuration for ProjectionHealthChecker
 */
export interface HealthCheckerConfig {
  /** Checkpoint store */
  checkpointStore: ProjectionCheckpointStore;

  /** Failed event store */
  failedEventStore: FailedEventStore;

  /** Position provider (event store) */
  positionProvider: PositionProvider;

  /** Health thresholds */
  thresholds?: HealthThresholds;
}

/**
 * Checks health of projections
 */
export class ProjectionHealthChecker {
  private readonly checkpointStore: ProjectionCheckpointStore;
  private readonly failedEventStore: FailedEventStore;
  private readonly positionProvider: PositionProvider;
  private readonly thresholds: HealthThresholds;

  constructor(config: HealthCheckerConfig) {
    this.checkpointStore = config.checkpointStore;
    this.failedEventStore = config.failedEventStore;
    this.positionProvider = config.positionProvider;
    this.thresholds = config.thresholds ?? DEFAULT_HEALTH_THRESHOLDS;
  }

  /**
   * Get health status for a specific projection
   */
  async getHealth(projectionName: string): Promise<ProjectionHealth> {
    const [checkpoint, currentHead, failedEvents] = await Promise.all([
      this.checkpointStore.getCheckpoint(projectionName),
      this.positionProvider.getCurrentPosition(),
      this.failedEventStore.getByProjection(projectionName, { status: 'pending', limit: 5 }),
    ]);

    const lastProcessedPosition = checkpoint?.globalPosition ?? 0;
    const lastProcessedAt = checkpoint?.updatedAt ?? new Date(0);
    const lag = currentHead - lastProcessedPosition;
    const failedEventCount = failedEvents.length;
    const recentErrors = failedEvents.map((e) => e.errorMessage);

    const status = this.calculateStatus(lag, failedEventCount, lastProcessedAt);

    return {
      projectionName,
      status,
      lastProcessedPosition,
      currentHeadPosition: currentHead,
      lag,
      lastProcessedAt,
      failedEventCount,
      recentErrors,
    };
  }

  /**
   * Get health status for all projections
   */
  async getAllHealth(): Promise<ProjectionHealth[]> {
    const checkpoints = await this.checkpointStore.getAllCheckpoints();
    const projectionNames = checkpoints.map((c) => c.projectionName);

    return Promise.all(projectionNames.map((name) => this.getHealth(name)));
  }

  /**
   * Check if all projections are healthy
   */
  async isHealthy(): Promise<boolean> {
    const allHealth = await this.getAllHealth();
    return allHealth.every((h) => h.status === 'healthy');
  }

  /**
   * Get summary of overall health
   */
  async getHealthSummary(): Promise<{
    overall: 'healthy' | 'degraded' | 'unhealthy';
    healthy: number;
    degraded: number;
    unhealthy: number;
    totalLag: number;
    totalFailedEvents: number;
  }> {
    const allHealth = await this.getAllHealth();

    let healthy = 0;
    let degraded = 0;
    let unhealthy = 0;
    let totalLag = 0;
    let totalFailedEvents = 0;

    for (const health of allHealth) {
      totalLag += health.lag;
      totalFailedEvents += health.failedEventCount;

      switch (health.status) {
        case 'healthy':
          healthy++;
          break;
        case 'degraded':
          degraded++;
          break;
        case 'unhealthy':
          unhealthy++;
          break;
      }
    }

    let overall: 'healthy' | 'degraded' | 'unhealthy';
    if (unhealthy > 0) {
      overall = 'unhealthy';
    } else if (degraded > 0) {
      overall = 'degraded';
    } else {
      overall = 'healthy';
    }

    return {
      overall,
      healthy,
      degraded,
      unhealthy,
      totalLag,
      totalFailedEvents,
    };
  }

  /**
   * Calculate health status based on metrics
   */
  private calculateStatus(
    lag: number,
    failedEventCount: number,
    lastProcessedAt: Date
  ): 'healthy' | 'degraded' | 'unhealthy' {
    // Any failed events means unhealthy
    if (failedEventCount > 0) {
      return 'unhealthy';
    }

    // Check lag thresholds
    if (lag >= this.thresholds.unhealthyLagThreshold) {
      return 'unhealthy';
    }

    if (lag >= this.thresholds.degradedLagThreshold) {
      return 'degraded';
    }

    // Check staleness
    const age = Date.now() - lastProcessedAt.getTime();
    if (age > this.thresholds.maxHealthyAgeMs && lag > 0) {
      return 'degraded';
    }

    return 'healthy';
  }
}

/**
 * Create a simple health check endpoint response
 */
export async function createHealthCheckResponse(
  checker: ProjectionHealthChecker
): Promise<{
  status: 'pass' | 'warn' | 'fail';
  checks: Record<string, { status: string; observedValue: number; observedUnit: string }>;
}> {
  const summary = await checker.getHealthSummary();

  const status = summary.overall === 'healthy' ? 'pass' : summary.overall === 'degraded' ? 'warn' : 'fail';

  return {
    status,
    checks: {
      'projections:lag': {
        status: summary.totalLag < 100 ? 'pass' : summary.totalLag < 1000 ? 'warn' : 'fail',
        observedValue: summary.totalLag,
        observedUnit: 'events',
      },
      'projections:failed': {
        status: summary.totalFailedEvents === 0 ? 'pass' : 'fail',
        observedValue: summary.totalFailedEvents,
        observedUnit: 'events',
      },
    },
  };
}
