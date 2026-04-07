/**
 * Tests for ProjectionHealthChecker
 *
 * Verifies health status calculation, summary aggregation,
 * and the extracted determineOverallStatus helper.
 */

import {
  ProjectionHealthChecker,
  HealthCheckerConfig,
  DEFAULT_HEALTH_THRESHOLDS,
} from '../src/projections/health/projection-health';

// ============================================================================
// MOCK FACTORIES
// ============================================================================

function createMockCheckpointStore(
  checkpoints: Array<{
    projectionName: string;
    globalPosition: number;
    updatedAt: Date;
  }> = []
) {
  return {
    getCheckpoint: jest.fn((name: string) => {
      const cp = checkpoints.find((c) => c.projectionName === name);
      return Promise.resolve(cp ?? null);
    }),
    getAllCheckpoints: jest.fn(() => Promise.resolve(checkpoints)),
    saveCheckpoint: jest.fn(),
  };
}

function createMockFailedEventStore(failedEvents: Array<{ errorMessage: string }> = []) {
  return {
    getByProjection: jest.fn(() => Promise.resolve(failedEvents)),
    save: jest.fn(),
    getById: jest.fn(),
    updateStatus: jest.fn(),
    getAll: jest.fn(),
    deleteById: jest.fn(),
    retryEvent: jest.fn(),
  };
}

function createMockPositionProvider(position: number) {
  return {
    getCurrentPosition: jest.fn(() => Promise.resolve(position)),
  };
}

function createChecker(
  checkpoints: Parameters<typeof createMockCheckpointStore>[0],
  failedEvents: Parameters<typeof createMockFailedEventStore>[0],
  headPosition: number,
  thresholds = DEFAULT_HEALTH_THRESHOLDS
): ProjectionHealthChecker {
  const config: HealthCheckerConfig = {
    checkpointStore: createMockCheckpointStore(checkpoints) as any,
    failedEventStore: createMockFailedEventStore(failedEvents) as any,
    positionProvider: createMockPositionProvider(headPosition),
    thresholds,
  };
  return new ProjectionHealthChecker(config);
}

// ============================================================================
// TESTS
// ============================================================================

describe('ProjectionHealthChecker', () => {
  describe('getHealth', () => {
    it('returns healthy when lag is 0 and no failures', async () => {
      const checker = createChecker(
        [{ projectionName: 'orders', globalPosition: 100, updatedAt: new Date() }],
        [],
        100
      );
      const health = await checker.getHealth('orders');
      expect(health.status).toBe('healthy');
      expect(health.lag).toBe(0);
      expect(health.failedEventCount).toBe(0);
    });

    it('returns degraded when lag exceeds degraded threshold', async () => {
      const checker = createChecker(
        [{ projectionName: 'orders', globalPosition: 50, updatedAt: new Date() }],
        [],
        200
      );
      const health = await checker.getHealth('orders');
      expect(health.status).toBe('degraded');
      expect(health.lag).toBe(150);
    });

    it('returns unhealthy when lag exceeds unhealthy threshold', async () => {
      const checker = createChecker(
        [{ projectionName: 'orders', globalPosition: 0, updatedAt: new Date() }],
        [],
        1500
      );
      const health = await checker.getHealth('orders');
      expect(health.status).toBe('unhealthy');
    });

    it('returns unhealthy when there are failed events', async () => {
      const checker = createChecker(
        [{ projectionName: 'orders', globalPosition: 100, updatedAt: new Date() }],
        [{ errorMessage: 'something broke' }],
        100
      );
      const health = await checker.getHealth('orders');
      expect(health.status).toBe('unhealthy');
      expect(health.failedEventCount).toBe(1);
    });

    it('returns degraded for stale projection with lag', async () => {
      const staleDate = new Date(Date.now() - 120_000);
      const checker = createChecker(
        [{ projectionName: 'orders', globalPosition: 99, updatedAt: staleDate }],
        [],
        100
      );
      const health = await checker.getHealth('orders');
      expect(health.status).toBe('degraded');
    });

    it('handles missing checkpoint gracefully', async () => {
      const checker = createChecker([], [], 100);
      const health = await checker.getHealth('nonexistent');
      expect(health.lastProcessedPosition).toBe(0);
      expect(health.lag).toBe(100);
    });
  });

  describe('getHealthSummary', () => {
    it('returns overall healthy when all projections healthy', async () => {
      const checker = createChecker(
        [
          { projectionName: 'orders', globalPosition: 100, updatedAt: new Date() },
          { projectionName: 'customers', globalPosition: 100, updatedAt: new Date() },
        ],
        [],
        100
      );
      const summary = await checker.getHealthSummary();
      expect(summary.overall).toBe('healthy');
      expect(summary.healthy).toBe(2);
      expect(summary.degraded).toBe(0);
      expect(summary.unhealthy).toBe(0);
      expect(summary.totalLag).toBe(0);
    });

    it('returns overall unhealthy when any projection is unhealthy', async () => {
      const checker = createChecker(
        [
          { projectionName: 'orders', globalPosition: 100, updatedAt: new Date() },
          { projectionName: 'customers', globalPosition: 0, updatedAt: new Date() },
        ],
        [{ errorMessage: 'failed' }],
        100
      );
      const summary = await checker.getHealthSummary();
      expect(summary.overall).toBe('unhealthy');
    });

    it('aggregates total lag across projections', async () => {
      const checker = createChecker(
        [
          { projectionName: 'orders', globalPosition: 90, updatedAt: new Date() },
          { projectionName: 'customers', globalPosition: 80, updatedAt: new Date() },
        ],
        [],
        100
      );
      const summary = await checker.getHealthSummary();
      expect(summary.totalLag).toBe(30);
    });
  });

  describe('isHealthy', () => {
    it('returns true when all projections are healthy', async () => {
      const checker = createChecker(
        [{ projectionName: 'orders', globalPosition: 100, updatedAt: new Date() }],
        [],
        100
      );
      expect(await checker.isHealthy()).toBe(true);
    });

    it('returns false when any projection is unhealthy', async () => {
      const checker = createChecker(
        [{ projectionName: 'orders', globalPosition: 100, updatedAt: new Date() }],
        [{ errorMessage: 'boom' }],
        100
      );
      expect(await checker.isHealthy()).toBe(false);
    });
  });
});
