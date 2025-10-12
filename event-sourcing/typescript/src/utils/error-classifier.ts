/**
 * Error classification utilities for event sourcing operations
 *
 * Provides centralized error detection and classification for different error sources:
 * - gRPC status codes
 * - HTTP status codes
 * - Memory store errors
 * - Custom error types
 */

import { ConcurrencyConflictError } from '../core/errors';

/** gRPC status codes that indicate concurrency conflicts */
const GRPC_CONCURRENCY_CODES = ['FAILED_PRECONDITION', 'ABORTED'];

/** Message patterns that indicate concurrency conflicts */
const CONCURRENCY_MESSAGE_PATTERNS = [
  'concurrency',
  'expected version',
  'precondition',
  'version mismatch',
  'optimistic lock',
];

/**
 * Check if an error represents a concurrency conflict
 *
 * This function handles errors from multiple sources:
 * - ConcurrencyConflictError instances
 * - gRPC errors with FAILED_PRECONDITION or ABORTED codes
 * - Errors with concurrency-related messages
 *
 * @param error - The error to classify
 * @returns true if the error is a concurrency conflict
 */
export function isConcurrencyError(error: unknown): boolean {
  if (!error) {
    return false;
  }

  // Check for custom ConcurrencyConflictError
  if (error instanceof ConcurrencyConflictError) {
    return true;
  }

  // Check error message patterns
  if (error instanceof Error) {
    const message = error.message.toLowerCase();
    return CONCURRENCY_MESSAGE_PATTERNS.some((pattern) => message.includes(pattern));
  }

  // Check gRPC status codes
  if (typeof error === 'object' && error !== null) {
    const code = (error as { code?: unknown }).code;
    if (typeof code === 'string') {
      return GRPC_CONCURRENCY_CODES.includes(code.toUpperCase());
    }
  }

  return false;
}

/**
 * Check if an error represents a not found condition
 *
 * @param error - The error to classify
 * @returns true if the error indicates a resource was not found
 */
export function isNotFoundError(error: unknown): boolean {
  if (!error) {
    return false;
  }

  if (error instanceof Error) {
    const message = error.message.toLowerCase();
    return message.includes('not found') || message.includes('does not exist');
  }

  if (typeof error === 'object' && error !== null) {
    const code = (error as { code?: unknown }).code;
    if (typeof code === 'string') {
      return code.toUpperCase() === 'NOT_FOUND';
    }
  }

  return false;
}

/**
 * Check if an error represents a validation failure
 *
 * @param error - The error to classify
 * @returns true if the error indicates validation failure
 */
export function isValidationError(error: unknown): boolean {
  if (!error) {
    return false;
  }

  if (error instanceof Error) {
    const message = error.message.toLowerCase();
    return (
      message.includes('validation') || message.includes('invalid') || message.includes('required')
    );
  }

  if (typeof error === 'object' && error !== null) {
    const code = (error as { code?: unknown }).code;
    if (typeof code === 'string') {
      return code.toUpperCase() === 'INVALID_ARGUMENT';
    }
  }

  return false;
}
