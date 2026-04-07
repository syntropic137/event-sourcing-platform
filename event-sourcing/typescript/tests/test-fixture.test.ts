/**
 * Tests for test-fixture: loadFixture and createFixture helpers
 */

import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { loadFixture, createFixture } from '../src/testing/fixtures/test-fixture';

// ---------------------------------------------------------------------------
// Temp directory setup / teardown
// ---------------------------------------------------------------------------

const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'test-fixture-tests-'));

afterAll(() => {
  fs.rmSync(tmpDir, { recursive: true, force: true });
});

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function writeFixtureFile(name: string, content: object): string {
  const filePath = path.join(tmpDir, name);
  fs.writeFileSync(filePath, JSON.stringify(content, null, 2), 'utf-8');
  return filePath;
}

const VALID_FIXTURE = {
  description: 'Test fixture',
  aggregateType: 'TestAggregate',
  events: [{ type: 'TestEvent', version: 'v1', data: { field: 'value' } }],
};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('loadFixture', () => {
  it('loads a valid JSON fixture file', async () => {
    const filePath = writeFixtureFile('valid.json', VALID_FIXTURE);

    const loaded = await loadFixture(filePath);

    expect(loaded.description).toBe('Test fixture');
    expect(loaded.aggregateType).toBe('TestAggregate');
    expect(loaded.events).toHaveLength(1);
    expect(loaded.events[0].type).toBe('TestEvent');
    expect(loaded.format).toBe('json');
    expect(loaded.filePath).toBe(filePath);
  });

  it('throws for a missing file', async () => {
    const missingPath = path.join(tmpDir, 'does-not-exist.json');

    await expect(loadFixture(missingPath)).rejects.toThrow('Fixture file not found');
  });

  it('throws for an unsupported file extension', async () => {
    const filePath = path.join(tmpDir, 'bad.xml');
    fs.writeFileSync(filePath, '<root/>', 'utf-8');

    await expect(loadFixture(filePath)).rejects.toThrow('Unsupported fixture format');
  });

  it('skips validation when validate: false', async () => {
    // Write a fixture missing required fields — normally would fail validation
    const filePath = writeFixtureFile('no-validate.json', {
      events: [{ type: 'E', version: 'v1', data: {} }],
    });

    const loaded = await loadFixture(filePath, { validate: false });

    // Should succeed despite missing description / aggregateType
    expect(loaded.events).toHaveLength(1);
    expect(loaded.format).toBe('json');
  });

  it('throws a FixtureValidationError for invalid fixture structure', async () => {
    const filePath = writeFixtureFile('invalid.json', {
      description: 123, // wrong type
      events: 'not-an-array',
    });

    await expect(loadFixture(filePath)).rejects.toThrow('Invalid fixture');
  });

  it('resolves relative paths using baseDir option', async () => {
    writeFixtureFile('relative.json', VALID_FIXTURE);

    const loaded = await loadFixture('relative.json', { baseDir: tmpDir });

    expect(loaded.description).toBe('Test fixture');
    expect(loaded.filePath).toBe(path.join(tmpDir, 'relative.json'));
  });
});

describe('createFixture', () => {
  it('creates a fixture with defaults', () => {
    const fixture = createFixture({
      events: [{ type: 'Created', version: 'v1', data: { id: '1' } }],
    });

    expect(fixture.description).toBe('Generated fixture');
    expect(fixture.aggregateType).toBe('Unknown');
    expect(fixture.events).toHaveLength(1);
    expect(fixture.expectedState).toBeUndefined();
    expect(fixture.expectedVersion).toBeUndefined();
    expect(fixture.tags).toBeUndefined();
  });

  it('uses provided values instead of defaults', () => {
    const fixture = createFixture({
      description: 'Custom description',
      aggregateType: 'Order',
      events: [{ type: 'OrderPlaced', version: 'v1', data: { total: 42 } }],
      expectedVersion: 1,
      tags: ['smoke'],
    });

    expect(fixture.description).toBe('Custom description');
    expect(fixture.aggregateType).toBe('Order');
    expect(fixture.expectedVersion).toBe(1);
    expect(fixture.tags).toEqual(['smoke']);
  });
});
