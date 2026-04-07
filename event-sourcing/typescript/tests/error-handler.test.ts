/**
 * Tests for ProjectionErrorHandler (executeWithRetry, handleError)
 */

import { ProjectionErrorHandler } from '../src/projections/failure/error-handler';
import { RetryPolicy } from '../src/projections/failure/retry-policy';
import { ProjectionResult } from '../src/projections/types';
import type { FailedEventStore } from '../src/projections/failure/failed-event-store';

// Mock sleep to avoid real delays
jest.mock('../src/projections/failure/retry-policy', () => {
  const actual = jest.requireActual('../src/projections/failure/retry-policy');
  return {
    ...actual,
    sleep: jest.fn().mockResolvedValue(undefined),
  };
});

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function createMockFailedEventStore(): FailedEventStore {
  return {
    save: jest.fn().mockResolvedValue(undefined),
    getByProjection: jest.fn().mockResolvedValue([]),
    getByStatus: jest.fn().mockResolvedValue([]),
    getById: jest.fn().mockResolvedValue(null),
    markReprocessing: jest.fn().mockResolvedValue(undefined),
    markResolved: jest.fn().mockResolvedValue(undefined),
    markIgnored: jest.fn().mockResolvedValue(undefined),
    getCountByProjection: jest.fn().mockResolvedValue(0),
    getPendingCount: jest.fn().mockResolvedValue(0),
    cleanupResolved: jest.fn().mockResolvedValue(0),
  };
}

const mockEnvelope = {
  event: { eventType: 'TestEvent', schemaVersion: 1, toJson: () => ({}) },
  metadata: {
    eventId: 'evt-1',
    aggregateId: 'agg-1',
    aggregateType: 'Test',
    aggregateNonce: 1,
    timestamp: new Date().toISOString(),
    recordedTimestamp: new Date().toISOString(),
    contentType: 'application/json',
    headers: {},
    customMetadata: {},
    globalNonce: 1,
  },
} as any;

const PROJECTION_NAME = 'TestProjection';

// ---------------------------------------------------------------------------
// Tests: executeWithRetry
// ---------------------------------------------------------------------------

describe('ProjectionErrorHandler', () => {
  describe('executeWithRetry', () => {
    it('returns immediately with retryCount=0 on SUCCESS', async () => {
      const store = createMockFailedEventStore();
      const handler = new ProjectionErrorHandler({
        retryPolicy: RetryPolicy.fixedDelay(10, 3),
        failedEventStore: store,
      });

      const result = await handler.executeWithRetry(
        mockEnvelope,
        PROJECTION_NAME,
        async () => ProjectionResult.SUCCESS
      );

      expect(result.result).toBe(ProjectionResult.SUCCESS);
      expect(result.retryCount).toBe(0);
      expect(result.sentToDLQ).toBe(false);
      expect(store.save).not.toHaveBeenCalled();
    });

    it('returns immediately on SKIP result', async () => {
      const store = createMockFailedEventStore();
      const handler = new ProjectionErrorHandler({
        retryPolicy: RetryPolicy.fixedDelay(10, 3),
        failedEventStore: store,
      });

      const result = await handler.executeWithRetry(
        mockEnvelope,
        PROJECTION_NAME,
        async () => ProjectionResult.SKIP
      );

      expect(result.result).toBe(ProjectionResult.SKIP);
      expect(result.retryCount).toBe(0);
      expect(result.sentToDLQ).toBe(false);
    });

    it('sends to DLQ on FAILURE result', async () => {
      const store = createMockFailedEventStore();
      const handler = new ProjectionErrorHandler({
        retryPolicy: RetryPolicy.fixedDelay(10, 3),
        failedEventStore: store,
      });

      const result = await handler.executeWithRetry(
        mockEnvelope,
        PROJECTION_NAME,
        async () => ProjectionResult.FAILURE
      );

      expect(result.result).toBe(ProjectionResult.FAILURE);
      expect(result.sentToDLQ).toBe(true);
      expect(store.save).toHaveBeenCalledTimes(1);
    });

    it('retries on RETRY result then succeeds', async () => {
      const store = createMockFailedEventStore();
      const handler = new ProjectionErrorHandler({
        retryPolicy: new RetryPolicy({
          maxRetries: 3,
          initialDelayMs: 10,
          maxDelayMs: 100,
          backoffMultiplier: 1,
          retryablePatterns: ['retry'],
        }),
        failedEventStore: store,
      });

      let callCount = 0;
      const result = await handler.executeWithRetry(mockEnvelope, PROJECTION_NAME, async () => {
        callCount++;
        return callCount < 3 ? ProjectionResult.RETRY : ProjectionResult.SUCCESS;
      });

      expect(result.result).toBe(ProjectionResult.SUCCESS);
      expect(result.retryCount).toBe(2);
      expect(result.sentToDLQ).toBe(false);
      expect(callCount).toBe(3);
    });

    it('retries on thrown error then succeeds on second attempt', async () => {
      const store = createMockFailedEventStore();
      const handler = new ProjectionErrorHandler({
        retryPolicy: new RetryPolicy({
          maxRetries: 3,
          initialDelayMs: 10,
          maxDelayMs: 100,
          backoffMultiplier: 1,
          retryablePatterns: ['connection'],
        }),
        failedEventStore: store,
      });

      let callCount = 0;
      const result = await handler.executeWithRetry(mockEnvelope, PROJECTION_NAME, async () => {
        callCount++;
        if (callCount === 1) throw new Error('connection refused');
        return ProjectionResult.SUCCESS;
      });

      expect(result.result).toBe(ProjectionResult.SUCCESS);
      expect(result.retryCount).toBe(1);
      expect(result.sentToDLQ).toBe(false);
    });

    it('sends to DLQ after exhausting retries', async () => {
      const store = createMockFailedEventStore();
      const handler = new ProjectionErrorHandler({
        retryPolicy: new RetryPolicy({
          maxRetries: 2,
          initialDelayMs: 10,
          maxDelayMs: 100,
          backoffMultiplier: 1,
          retryablePatterns: ['retry'],
        }),
        failedEventStore: store,
      });

      let callCount = 0;
      const result = await handler.executeWithRetry(mockEnvelope, PROJECTION_NAME, async () => {
        callCount++;
        return ProjectionResult.RETRY;
      });

      expect(result.result).toBe(ProjectionResult.FAILURE);
      expect(result.sentToDLQ).toBe(true);
      expect(store.save).toHaveBeenCalled();
      // Called once for initial + maxRetries(2) times retried = 3 total
      expect(callCount).toBe(3);
    });
  });

  // ---------------------------------------------------------------------------
  // Tests: handleError
  // ---------------------------------------------------------------------------

  describe('handleError', () => {
    it('sends non-retryable error to DLQ immediately', async () => {
      const store = createMockFailedEventStore();
      const handler = new ProjectionErrorHandler({
        retryPolicy: new RetryPolicy({
          maxRetries: 3,
          initialDelayMs: 10,
          maxDelayMs: 100,
          backoffMultiplier: 1,
          // No retryable patterns → non-transient errors go straight to DLQ
        }),
        failedEventStore: store,
      });

      const error = new Error('some permanent error');
      const { shouldRetry, delayMs } = await handler.handleError(
        mockEnvelope,
        PROJECTION_NAME,
        error,
        0
      );

      expect(shouldRetry).toBe(false);
      expect(delayMs).toBe(0);
      expect(store.save).toHaveBeenCalledTimes(1);
    });

    it('returns shouldRetry=true with delay for retryable error', async () => {
      const store = createMockFailedEventStore();
      const handler = new ProjectionErrorHandler({
        retryPolicy: new RetryPolicy({
          maxRetries: 3,
          initialDelayMs: 100,
          maxDelayMs: 5000,
          backoffMultiplier: 2,
          retryablePatterns: ['timeout'],
        }),
        failedEventStore: store,
      });

      const error = new Error('request timeout');
      const { shouldRetry, delayMs } = await handler.handleError(
        mockEnvelope,
        PROJECTION_NAME,
        error,
        0
      );

      expect(shouldRetry).toBe(true);
      expect(delayMs).toBeGreaterThan(0);
      expect(store.save).not.toHaveBeenCalled();
    });
  });
});
