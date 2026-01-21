import {
  parseManifest,
  validateManifest,
  hasDomainData,
  hasBoundedContexts,
  getContextCount,
} from '../../src/manifest/parser';
import { ManifestValidationError } from '../../src/types/manifest';
import * as fs from 'fs';
import * as path from 'path';

describe('Manifest Parser', () => {
  const testManifestPath = path.join(__dirname, '..', 'fixtures', 'test-manifest.json');
  let testManifestJson: string;

  beforeAll(() => {
    testManifestJson = fs.readFileSync(testManifestPath, 'utf-8');
  });

  describe('parseManifest', () => {
    it('should parse valid manifest JSON', () => {
      const manifest = parseManifest(testManifestJson);

      expect(manifest.version).toBe('0.6.1-beta');
      expect(manifest.schema_version).toBe('1.0.0');
      expect(manifest.domain).toBeDefined();
    });

    it('should throw error for invalid JSON', () => {
      expect(() => parseManifest('not valid json')).toThrow(ManifestValidationError);
    });

    it('should parse domain manifest correctly', () => {
      const manifest = parseManifest(testManifestJson);

      expect(manifest.domain).toBeDefined();
      expect(manifest.domain!.aggregates).toHaveLength(1);
      expect(manifest.domain!.commands).toHaveLength(2);
      expect(manifest.domain!.events).toHaveLength(2);
    });

    it('should parse aggregate details correctly', () => {
      const manifest = parseManifest(testManifestJson);
      const aggregate = manifest.domain!.aggregates[0];

      expect(aggregate.name).toBe('CartAggregate');
      expect(aggregate.file_path).toBe('src/domain/cart/CartAggregate.ts');
      expect(aggregate.command_handlers).toHaveLength(2);
      expect(aggregate.event_handlers).toHaveLength(2);
    });

    it('should parse command handlers with emitted events', () => {
      const manifest = parseManifest(testManifestJson);
      const handler = manifest.domain!.aggregates[0].command_handlers[0];

      expect(handler.command_type).toBe('CreateCart');
      expect(handler.emits_events).toEqual(['CartCreated']);
    });

    it('should parse relationships correctly', () => {
      const manifest = parseManifest(testManifestJson);
      const relationships = manifest.domain!.relationships;

      expect(relationships.command_to_aggregate['CreateCart']).toBe('CartAggregate');
      expect(relationships.aggregate_to_events['CartAggregate']).toEqual([
        'CartCreated',
        'ItemAddedToCart',
      ]);
      expect(relationships.event_to_handlers['CartCreated']).toEqual(['CartAggregate']);
    });
  });

  describe('validateManifest', () => {
    it('should validate manifest with required fields', () => {
      const validManifest = {
        version: '0.6.1-beta',
        schema_version: '1.0.0',
        generated_at: '2026-01-21T00:00:00Z',
      };

      expect(() => validateManifest(validManifest)).not.toThrow();
    });

    it('should throw error for missing version', () => {
      const invalid = {
        schema_version: '1.0.0',
        generated_at: '2026-01-21T00:00:00Z',
      };

      expect(() => validateManifest(invalid)).toThrow(ManifestValidationError);
      expect(() => validateManifest(invalid)).toThrow('version');
    });

    it('should throw error for missing schema_version', () => {
      const invalid = {
        version: '0.6.1-beta',
        generated_at: '2026-01-21T00:00:00Z',
      };

      expect(() => validateManifest(invalid)).toThrow(ManifestValidationError);
      expect(() => validateManifest(invalid)).toThrow('schema_version');
    });

    it('should throw error for non-object input', () => {
      expect(() => validateManifest(null)).toThrow(ManifestValidationError);
      expect(() => validateManifest('string')).toThrow(ManifestValidationError);
      expect(() => validateManifest(123)).toThrow(ManifestValidationError);
    });

    it('should validate domain structure when present', () => {
      const withInvalidDomain = {
        version: '0.6.1-beta',
        schema_version: '1.0.0',
        generated_at: '2026-01-21T00:00:00Z',
        domain: {
          aggregates: 'not an array',
        },
      };

      expect(() => validateManifest(withInvalidDomain)).toThrow(ManifestValidationError);
    });

    it('should require relationships in domain', () => {
      const withoutRelationships = {
        version: '0.6.1-beta',
        schema_version: '1.0.0',
        generated_at: '2026-01-21T00:00:00Z',
        domain: {
          aggregates: [],
          commands: [],
          events: [],
        },
      };

      expect(() => validateManifest(withoutRelationships)).toThrow(ManifestValidationError);
      expect(() => validateManifest(withoutRelationships)).toThrow('relationships');
    });
  });

  describe('hasDomainData', () => {
    it('should return true for manifest with domain data', () => {
      const manifest = parseManifest(testManifestJson);
      expect(hasDomainData(manifest)).toBe(true);
    });

    it('should return false for manifest without domain', () => {
      const minimal = {
        version: '0.6.1-beta',
        schema_version: '1.0.0',
        generated_at: '2026-01-21T00:00:00Z',
      };

      const manifest = validateManifest(minimal);
      expect(hasDomainData(manifest)).toBe(false);
    });

    it('should return false for manifest with empty aggregates', () => {
      const emptyDomain = {
        version: '0.6.1-beta',
        schema_version: '1.0.0',
        generated_at: '2026-01-21T00:00:00Z',
        domain: {
          aggregates: [],
          commands: [],
          events: [],
          relationships: {
            command_to_aggregate: {},
            aggregate_to_events: {},
            event_to_handlers: {},
          },
        },
      };

      const manifest = validateManifest(emptyDomain);
      expect(hasDomainData(manifest)).toBe(false);
    });
  });

  describe('hasBoundedContexts', () => {
    it('should return false for manifest without bounded contexts', () => {
      const manifest = parseManifest(testManifestJson);
      expect(hasBoundedContexts(manifest)).toBe(false);
    });

    it('should return true for manifest with bounded contexts', () => {
      const withContexts = {
        version: '0.6.1-beta',
        schema_version: '1.0.0',
        generated_at: '2026-01-21T00:00:00Z',
        bounded_contexts: [
          {
            name: 'Cart',
            aggregates: ['CartAggregate'],
            publishes: ['CartCreated'],
            subscribes: [],
          },
        ],
      };

      const manifest = validateManifest(withContexts);
      expect(hasBoundedContexts(manifest)).toBe(true);
    });
  });

  describe('getContextCount', () => {
    it('should return 0 for manifest without bounded contexts', () => {
      const manifest = parseManifest(testManifestJson);
      expect(getContextCount(manifest)).toBe(0);
    });

    it('should return correct count for manifest with bounded contexts', () => {
      const withContexts = {
        version: '0.6.1-beta',
        schema_version: '1.0.0',
        generated_at: '2026-01-21T00:00:00Z',
        bounded_contexts: [
          { name: 'Cart', aggregates: [], publishes: [], subscribes: [] },
          { name: 'Order', aggregates: [], publishes: [], subscribes: [] },
        ],
      };

      const manifest = validateManifest(withContexts);
      expect(getContextCount(manifest)).toBe(2);
    });
  });
});
