/**
 * Error types for scenario testing
 */

/**
 * Error thrown when a scenario assertion fails
 */
export class ScenarioAssertionError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'ScenarioAssertionError';

    // Ensure the error stack trace points to where the error was thrown
    if (Error.captureStackTrace) {
      Error.captureStackTrace(this, this.constructor);
    }
  }
}

/**
 * Error thrown when scenario execution fails unexpectedly
 */
export class ScenarioExecutionError extends Error {
  constructor(
    message: string,
    public readonly originalError?: Error
  ) {
    super(message);
    this.name = 'ScenarioExecutionError';

    if (Error.captureStackTrace) {
      Error.captureStackTrace(this, this.constructor);
    }
  }
}
