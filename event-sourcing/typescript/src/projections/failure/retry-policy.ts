/**
 * Retry policy for projection error handling
 */

import { RetryPolicyConfig, DEFAULT_RETRY_POLICY } from '../types';

/**
 * Retry policy implementation
 *
 * Provides exponential backoff with configurable limits.
 */
export class RetryPolicy {
  private readonly config: RetryPolicyConfig;

  constructor(config: Partial<RetryPolicyConfig> = {}) {
    this.config = { ...DEFAULT_RETRY_POLICY, ...config };
  }

  /**
   * Calculate delay for a given attempt number
   *
   * @param attempt - Attempt number (1-based)
   * @returns Delay in milliseconds
   */
  getDelay(attempt: number): number {
    const { initialDelayMs, maxDelayMs, backoffMultiplier } = this.config;
    const delay = initialDelayMs * Math.pow(backoffMultiplier, attempt - 1);
    return Math.min(delay, maxDelayMs);
  }

  /**
   * Check if another retry is allowed
   *
   * @param attemptCount - Number of attempts made so far
   * @returns Whether to retry
   */
  shouldRetry(attemptCount: number): boolean {
    return attemptCount < this.config.maxRetries;
  }

  /**
   * Check if an error is retryable
   *
   * @param error - The error to check
   * @returns Whether the error is retryable
   */
  isRetryable(error: Error): boolean {
    // Check custom patterns first
    if (this.config.retryablePatterns && this.config.retryablePatterns.length > 0) {
      const message = error.message.toLowerCase();
      return this.config.retryablePatterns.some((pattern) =>
        message.includes(pattern.toLowerCase())
      );
    }

    // Default transient error detection
    return this.isTransientError(error);
  }

  /**
   * Check if an error appears to be transient (network, timeout, etc.)
   */
  private isTransientError(error: Error): boolean {
    const transientPatterns = [
      'econnrefused',
      'etimedout',
      'econnreset',
      'enotfound',
      'connection',
      'timeout',
      'unavailable',
      'temporarily',
      'overloaded',
      'too many connections',
      'connection pool',
      'socket hang up',
    ];

    const message = error.message.toLowerCase();
    return transientPatterns.some((pattern) => message.includes(pattern));
  }

  /**
   * Get the maximum number of retries
   */
  get maxRetries(): number {
    return this.config.maxRetries;
  }

  /**
   * Create a policy with exponential backoff (default)
   */
  static exponentialBackoff(options?: Partial<RetryPolicyConfig>): RetryPolicy {
    return new RetryPolicy(options);
  }

  /**
   * Create a policy with no retries
   */
  static noRetry(): RetryPolicy {
    return new RetryPolicy({ maxRetries: 0 });
  }

  /**
   * Create a policy with fixed delay
   */
  static fixedDelay(delayMs: number, maxRetries = 3): RetryPolicy {
    return new RetryPolicy({
      initialDelayMs: delayMs,
      maxDelayMs: delayMs,
      backoffMultiplier: 1,
      maxRetries,
    });
  }
}

/**
 * Sleep for a given number of milliseconds
 */
export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
