/**
 * Fixture loading and management utilities
 */

import * as fs from 'fs';
import * as path from 'path';
import {
  TestFixture,
  LoadedFixture,
  LoadFixtureOptions,
  validateFixture,
  FixtureValidationError,
} from './fixture-types';

/**
 * Parse fixture content based on file extension
 */
function parseFixtureContent(
  content: string,
  ext: string,
  resolvedPath: string
): { parsed: unknown; format: 'json' | 'yaml' } {
  if (ext === '.json') {
    try {
      return { parsed: JSON.parse(content), format: 'json' };
    } catch {
      throw new Error(`Invalid JSON in fixture file: ${resolvedPath}`);
    }
  }

  if (ext === '.yaml' || ext === '.yml') {
    try {
      // eslint-disable-next-line @typescript-eslint/no-require-imports
      const yaml = require('js-yaml');
      return { parsed: yaml.load(content), format: 'yaml' };
    } catch (error) {
      if ((error as Error).message?.includes("Cannot find module 'js-yaml'")) {
        throw new Error('YAML fixtures require js-yaml package. Install with: npm install js-yaml');
      }
      throw new Error(`Invalid YAML in fixture file: ${resolvedPath}`);
    }
  }

  throw new Error(`Unsupported fixture format: ${ext}. Use .json or .yaml/.yml`);
}

/**
 * Load a fixture from a JSON or YAML file
 *
 * @param filePath - Path to the fixture file
 * @param options - Loading options
 * @returns Loaded and validated fixture
 *
 * @example
 * ```typescript
 * const fixture = await loadFixture('./fixtures/order-lifecycle.json');
 * ```
 */
export async function loadFixture(
  filePath: string,
  options: LoadFixtureOptions = {}
): Promise<LoadedFixture> {
  const { baseDir = process.cwd(), validate = true } = options;
  const resolvedPath = path.isAbsolute(filePath) ? filePath : path.join(baseDir, filePath);

  if (!fs.existsSync(resolvedPath)) {
    throw new Error(`Fixture file not found: ${resolvedPath}`);
  }

  let content: string;
  try {
    content = fs.readFileSync(resolvedPath, 'utf-8');
  } catch {
    throw new Error(`Failed to read fixture file: ${resolvedPath}`);
  }

  const ext = path.extname(resolvedPath).toLowerCase();
  const { parsed, format } = parseFixtureContent(content, ext, resolvedPath);
  const fixture = validate ? validateFixture(parsed, resolvedPath) : (parsed as TestFixture);

  return { ...fixture, filePath: resolvedPath, format };
}

/**
 * Load multiple fixtures from a directory
 *
 * @param dirPath - Path to directory containing fixtures
 * @param options - Loading options
 * @returns Array of loaded fixtures
 *
 * @example
 * ```typescript
 * const fixtures = await loadFixturesFromDirectory('./fixtures');
 * ```
 */
export async function loadFixturesFromDirectory(
  dirPath: string,
  options: LoadFixtureOptions = {}
): Promise<LoadedFixture[]> {
  const { baseDir = process.cwd() } = options;
  const resolvedPath = path.isAbsolute(dirPath) ? dirPath : path.join(baseDir, dirPath);

  if (!fs.existsSync(resolvedPath)) {
    throw new Error(`Fixture directory not found: ${resolvedPath}`);
  }

  const files = fs.readdirSync(resolvedPath);
  const fixtureFiles = files.filter((f) => {
    const ext = path.extname(f).toLowerCase();
    return ext === '.json' || ext === '.yaml' || ext === '.yml';
  });

  const fixtures: LoadedFixture[] = [];

  for (const file of fixtureFiles) {
    try {
      const fixture = await loadFixture(path.join(resolvedPath, file), options);
      fixtures.push(fixture);
    } catch (error) {
      if (error instanceof FixtureValidationError) {
        throw error; // Re-throw validation errors
      }
      // Skip non-fixture files silently
      console.warn(`Skipping file ${file}: ${(error as Error).message}`);
    }
  }

  return fixtures;
}

/**
 * Load fixtures matching specific tags
 *
 * @param dirPath - Path to directory containing fixtures
 * @param tags - Tags to filter by (OR logic)
 * @param options - Loading options
 * @returns Array of fixtures matching any of the tags
 */
export async function loadFixturesByTags(
  dirPath: string,
  tags: string[],
  options: LoadFixtureOptions = {}
): Promise<LoadedFixture[]> {
  const allFixtures = await loadFixturesFromDirectory(dirPath, options);

  return allFixtures.filter((fixture) => {
    if (!fixture.tags || fixture.tags.length === 0) {
      return false;
    }
    return tags.some((tag) => fixture.tags?.includes(tag));
  });
}

/**
 * Create a fixture programmatically (useful for generating fixtures from tests)
 */
export function createFixture(
  partial: Partial<TestFixture> & { events: TestFixture['events'] }
): TestFixture {
  return {
    description: partial.description ?? 'Generated fixture',
    aggregateType: partial.aggregateType ?? 'Unknown',
    events: partial.events,
    expectedState: partial.expectedState,
    expectedVersion: partial.expectedVersion,
    aggregateId: partial.aggregateId,
    tags: partial.tags,
  };
}

/**
 * Save a fixture to a file (useful for generating fixtures from production events)
 */
export async function saveFixture(
  fixture: TestFixture,
  filePath: string,
  options: { format?: 'json' | 'yaml'; pretty?: boolean } = {}
): Promise<void> {
  const { format = 'json', pretty = true } = options;

  let content: string;

  if (format === 'json') {
    content = pretty ? JSON.stringify(fixture, null, 2) : JSON.stringify(fixture);
  } else {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const yaml = require('js-yaml');
    content = yaml.dump(fixture);
  }

  const dir = path.dirname(filePath);
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }

  fs.writeFileSync(filePath, content, 'utf-8');
}
