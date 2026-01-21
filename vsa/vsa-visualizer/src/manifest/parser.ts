import { Manifest, ManifestValidationError } from '../types/manifest';

/**
 * Parse and validate a VSA manifest from JSON string
 */
export function parseManifest(jsonString: string): Manifest {
  let parsed: unknown;

  try {
    parsed = JSON.parse(jsonString);
  } catch (error) {
    throw new ManifestValidationError(
      `Invalid JSON: ${error instanceof Error ? error.message : 'Unknown error'}`
    );
  }

  return validateManifest(parsed);
}

/**
 * Validate that the parsed object conforms to Manifest schema
 */
export function validateManifest(obj: unknown): Manifest {
  if (typeof obj !== 'object' || obj === null) {
    throw new ManifestValidationError('Manifest must be an object');
  }

  const manifest = obj as Record<string, unknown>;

  // Check required fields
  if (typeof manifest.version !== 'string') {
    throw new ManifestValidationError('Manifest must have a version string');
  }

  if (typeof manifest.schema_version !== 'string') {
    throw new ManifestValidationError('Manifest must have a schema_version string');
  }

  if (typeof manifest.generated_at !== 'string') {
    throw new ManifestValidationError('Manifest must have a generated_at string');
  }

  // Validate domain if present
  if (manifest.domain !== undefined) {
    validateDomain(manifest.domain);
  }

  // Validate bounded_contexts if present
  if (manifest.bounded_contexts !== undefined) {
    if (!Array.isArray(manifest.bounded_contexts)) {
      throw new ManifestValidationError('bounded_contexts must be an array');
    }
  }

  // TypeScript type assertion - we've validated all required fields above
  return manifest as unknown as Manifest;
}

/**
 * Validate domain manifest structure
 */
function validateDomain(domain: unknown): void {
  if (typeof domain !== 'object' || domain === null) {
    throw new ManifestValidationError('Domain must be an object');
  }

  const domainObj = domain as Record<string, unknown>;

  // Check required arrays
  const requiredArrays = ['aggregates', 'commands', 'events'];
  for (const field of requiredArrays) {
    if (!Array.isArray(domainObj[field])) {
      throw new ManifestValidationError(`Domain.${field} must be an array`);
    }
  }

  // Check relationships
  if (typeof domainObj.relationships !== 'object' || domainObj.relationships === null) {
    throw new ManifestValidationError('Domain.relationships must be an object');
  }

  const relationships = domainObj.relationships as Record<string, unknown>;

  if (typeof relationships.command_to_aggregate !== 'object') {
    throw new ManifestValidationError(
      'Domain.relationships.command_to_aggregate must be an object'
    );
  }

  if (typeof relationships.aggregate_to_events !== 'object') {
    throw new ManifestValidationError('Domain.relationships.aggregate_to_events must be an object');
  }

  if (typeof relationships.event_to_handlers !== 'object') {
    throw new ManifestValidationError('Domain.relationships.event_to_handlers must be an object');
  }
}

/**
 * Check if manifest supports domain visualization
 * 
 * Note: Domain data is optional. If not present, visualizer can still
 * generate context-level diagrams from bounded_contexts or contexts.
 */
export function hasDomainData(manifest: Manifest): boolean {
  return manifest.domain !== undefined && 
         (manifest.domain.aggregates.length > 0 ||
          manifest.domain.commands.length > 0 ||
          manifest.domain.events.length > 0);
}

/**
 * Check if manifest has bounded context data
 */
export function hasBoundedContexts(manifest: Manifest): boolean {
  return manifest.bounded_contexts !== undefined && manifest.bounded_contexts.length > 0;
}

/**
 * Get the number of bounded contexts
 */
export function getContextCount(manifest: Manifest): number {
  if (!manifest.bounded_contexts) {
    return 0;
  }
  return manifest.bounded_contexts.length;
}
