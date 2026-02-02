/**
 * Tests for Architecture SVG Generator
 */

import { ArchitectureSvgGenerator } from '../../src/generators/architecture-svg-generator';
import { Manifest } from '../../src/types/manifest';

describe('ArchitectureSvgGenerator', () => {
  const minimalManifest: Manifest = {
    version: '0.1.0',
    schema_version: '2.0.0',
    generated_at: '2026-01-25T00:00:00Z',
    bounded_contexts: []
  };

  describe('generate()', () => {
    it('should generate valid SVG document', () => {
      const generator = new ArchitectureSvgGenerator(minimalManifest);
      const svg = generator.generate();

      expect(svg).toContain('<svg');
      expect(svg).toContain('xmlns="http://www.w3.org/2000/svg"');
      expect(svg).toContain('</svg>');
    });

    it('should include title in header', () => {
      const generator = new ArchitectureSvgGenerator(minimalManifest);
      const svg = generator.generate();

      expect(svg).toContain('Architecture and Ecosystem');
    });

    it('should have correct viewBox dimensions', () => {
      const generator = new ArchitectureSvgGenerator(minimalManifest);
      const svg = generator.generate();

      expect(svg).toContain('viewBox="0 0 1400 1000"');
      expect(svg).toContain('width="1400"');
      expect(svg).toContain('height="1000"');
    });

    it('should include shadow filter definition', () => {
      const generator = new ArchitectureSvgGenerator(minimalManifest);
      const svg = generator.generate();

      expect(svg).toContain('<defs>');
      expect(svg).toContain('<filter id="shadow"');
      expect(svg).toContain('feGaussianBlur');
      expect(svg).toContain('</defs>');
    });
  });

  describe('with bounded contexts', () => {
    const manifestWithContexts: Manifest = {
      version: '0.1.0',
      schema_version: '2.0.0',
      generated_at: '2026-01-25T00:00:00Z',
      bounded_contexts: [
        {
          name: 'orders',
          path: './packages/domain/src/contexts/orders',
          features: [
            { name: 'place-order', path: 'slices/place-order', files: ['handler.ts'] },
            { name: 'cancel-order', path: 'slices/cancel-order', files: ['handler.ts'] }
          ]
        },
        {
          name: 'payments',
          path: './packages/domain/src/contexts/payments',
          features: [
            { name: 'process-payment', path: 'slices/process-payment', files: ['handler.ts'] }
          ]
        },
        {
          name: 'shipping',
          path: './packages/domain/src/contexts/shipping',
          features: []
        }
      ]
    };

    it('should render all bounded contexts', () => {
      const generator = new ArchitectureSvgGenerator(manifestWithContexts);
      const svg = generator.generate();

      expect(svg).toContain('orders');
      expect(svg).toContain('payments');
      expect(svg).toContain('shipping');
    });

    it('should show aggregate counts', () => {
      const generator = new ArchitectureSvgGenerator(manifestWithContexts);
      const svg = generator.generate();

      // Since no aggregate_count is specified, defaults to 0
      expect(svg).toContain('(0 aggregates)');
    });

    it('should render "Domain Contexts" section title', () => {
      const generator = new ArchitectureSvgGenerator(manifestWithContexts);
      const svg = generator.generate();

      expect(svg).toContain('Domain Contexts (Bounded Contexts)');
    });

    it('should use different colors for contexts', () => {
      const generator = new ArchitectureSvgGenerator(manifestWithContexts);
      const svg = generator.generate();

      // Check for the different fill colors (should cycle through 3 colors)
      expect(svg).toContain('fill="#d6c9ff"'); // Pastel purple
      expect(svg).toContain('fill="#c9d9ff"'); // Pastel blue
      expect(svg).toContain('fill="#ffd9e8"'); // Pastel pink
    });
  });

  describe('with layer configuration', () => {
    it('should render applications layer when configured', () => {
      const config = {
        applications: ['CLI', 'Dashboard', 'Web UI']
      };

      const generator = new ArchitectureSvgGenerator(minimalManifest, config);
      const svg = generator.generate();

      expect(svg).toContain('Applications');
      expect(svg).toContain('CLI');
      expect(svg).toContain('Dashboard');
      expect(svg).toContain('Web UI');
    });

    it('should render infrastructure layer when configured', () => {
      const config = {
        infrastructure: [
          { name: 'PostgreSQL', description: 'Event Store' },
          { name: 'Redis', description: 'Cache' }
        ]
      };

      const generator = new ArchitectureSvgGenerator(minimalManifest, config);
      const svg = generator.generate();

      expect(svg).toContain('Infrastructure');
      expect(svg).toContain('PostgreSQL');
      expect(svg).toContain('Event Store');
      expect(svg).toContain('Redis');
      expect(svg).toContain('Cache');
    });

    it('should render packages in sidebar when configured', () => {
      const config = {
        packages: ['aef-domain', 'aef-adapters', 'aef-collector']
      };

      const generator = new ArchitectureSvgGenerator(minimalManifest, config);
      const svg = generator.generate();

      expect(svg).toContain('Packages');
      expect(svg).toContain('aef-domain');
      expect(svg).toContain('aef-adapters');
      expect(svg).toContain('aef-collector');
    });

    it('should render libraries in sidebar when configured', () => {
      const config = {
        libraries: [
          { name: 'agentic-primitives', repo: 'github.com/org/agentic-primitives' },
          { name: 'event-sourcing-platform' }
        ]
      };

      const generator = new ArchitectureSvgGenerator(minimalManifest, config);
      const svg = generator.generate();

      expect(svg).toContain('Libraries');
      expect(svg).toContain('agentic-primitives');
      expect(svg).toContain('github.com/org/agentic-primitives');
      expect(svg).toContain('event-sourcing-platform');
    });

    it('should render complete architecture with all layers', () => {
      const manifestComplete: Manifest = {
        version: '0.1.0',
        schema_version: '2.0.0',
        generated_at: '2026-01-25T00:00:00Z',
        bounded_contexts: [
          {
            name: 'workflows',
            path: './packages/aef-domain/src/contexts/workflows',
            features: [
              { name: 'execute-workflow', path: 'slices/execute-workflow', files: [] },
              { name: 'create-workflow', path: 'slices/create-workflow', files: [] }
            ]
          },
          {
            name: 'workspaces',
            path: './packages/aef-domain/src/contexts/workspaces',
            features: [
              { name: 'create-workspace', path: 'slices/create-workspace', files: [] }
            ]
          }
        ]
      };

      const config = {
        applications: ['AEF CLI', 'Dashboard'],
        infrastructure: [
          { name: 'TimescaleDB', description: 'Projections' },
          { name: 'EventStore', description: 'Events' }
        ],
        packages: ['aef-domain', 'aef-adapters'],
        libraries: [
          { name: 'agentic-primitives' }
        ]
      };

      const generator = new ArchitectureSvgGenerator(manifestComplete, config);
      const svg = generator.generate();

      // Check all layers present
      expect(svg).toContain('Applications');
      expect(svg).toContain('Domain Contexts');
      expect(svg).toContain('Infrastructure');
      expect(svg).toContain('Packages');
      expect(svg).toContain('Libraries');

      // Check content
      expect(svg).toContain('AEF CLI');
      expect(svg).toContain('workflows');
      expect(svg).toContain('workspaces');
      expect(svg).toContain('TimescaleDB');
      expect(svg).toContain('aef-domain');
      expect(svg).toContain('agentic-primitives');
    });
  });

  describe('grid layout', () => {
    it('should layout contexts in 3-column grid', () => {
      const manifest: Manifest = {
        version: '0.1.0',
        schema_version: '2.0.0',
        generated_at: '2026-01-25T00:00:00Z',
        bounded_contexts: Array.from({ length: 9 }, (_, i) => ({
          name: `context-${i + 1}`,
          path: `./contexts/context-${i + 1}`,
          features: []
        }))
      };

      const generator = new ArchitectureSvgGenerator(manifest);
      const svg = generator.generate();

      // All 9 contexts should be present
      for (let i = 1; i <= 9; i++) {
        expect(svg).toContain(`context-${i}`);
      }
    });

    it('should handle single context', () => {
      const manifest: Manifest = {
        version: '0.1.0',
        schema_version: '2.0.0',
        generated_at: '2026-01-25T00:00:00Z',
        bounded_contexts: [
          {
            name: 'single-context',
            path: './contexts/single',
            features: [],
            aggregate_count: 2
          }
        ]
      };

      const generator = new ArchitectureSvgGenerator(manifest);
      const svg = generator.generate();

      expect(svg).toContain('single-context');
      expect(svg).toContain('(2 aggregates)');
    });
  });

  describe('feature filtering', () => {
    it('should exclude infrastructure folders from feature list', () => {
      const manifest: Manifest = {
        version: '0.1.0',
        schema_version: '2.2.0',
        generated_at: '2026-01-25T00:00:00Z',
        bounded_contexts: [
          {
            name: 'orders',
            path: './contexts/orders',
            aggregate_count: 1,
            features: [
              { name: 'domain', path: 'domain', files: [] },
              { name: 'slices', path: 'slices', files: [] },
              { name: 'place-order', path: 'slices/place-order', files: [], slice_type: 'command' },
              { name: 'cancel-order', path: 'slices/cancel-order', files: [], slice_type: 'command' }
            ]
          }
        ]
      };

      const generator = new ArchitectureSvgGenerator(manifest);
      const svg = generator.generate();

      // Should show aggregate count (new behavior)
      expect(svg).toContain('(1 aggregate)');
      // Should show actual features, not infrastructure folders
      expect(svg).toContain('place-order');
      expect(svg).toContain('cancel-order');
      expect(svg).not.toContain('>domain<');
      expect(svg).not.toContain('>slices<');
    });

    it('should show all features by default (no truncation)', () => {
      const features = Array.from({ length: 10 }, (_, i) => ({
        name: `feature-${i + 1}`,
        path: `slices/feature-${i + 1}`,
        files: [],
        slice_type: 'command' as const
      }));

      const manifest: Manifest = {
        version: '0.1.0',
        schema_version: '2.2.0',
        generated_at: '2026-01-25T00:00:00Z',
        bounded_contexts: [
          {
            name: 'large-context',
            path: './contexts/large',
            aggregate_count: 2,
            features
          }
        ]
      };

      const generator = new ArchitectureSvgGenerator(manifest);
      const svg = generator.generate();

      // Should show all 10 features (no truncation by default)
      for (let i = 1; i <= 10; i++) {
        expect(svg).toContain(`feature-${i}`);
      }
      // Should NOT have "... and X more" since no truncation
      expect(svg).not.toContain('... and');
    });
  });

  describe('SVG validity', () => {
    it('should escape special XML characters in text', () => {
      const manifest: Manifest = {
        version: '0.1.0',
        schema_version: '2.0.0',
        generated_at: '2026-01-25T00:00:00Z',
        bounded_contexts: [
          {
            name: 'test<>&"',
            path: './contexts/test',
            features: []
          }
        ]
      };

      const generator = new ArchitectureSvgGenerator(manifest);
      const svg = generator.generate();

      // Special characters should be escaped
      expect(svg).toContain('&lt;');
      expect(svg).toContain('&gt;');
      expect(svg).toContain('&amp;');
      expect(svg).toContain('&quot;');
    });

    it('should generate well-formed XML', () => {
      const generator = new ArchitectureSvgGenerator(minimalManifest);
      const svg = generator.generate();

      // Check basic XML structure
      const openTags = (svg.match(/<[^/][^>]*>/g) || []).length;
      const closeTags = (svg.match(/<\/[^>]+>/g) || []).length;

      // Basic sanity checks
      expect(openTags).toBeGreaterThan(0);
      expect(closeTags).toBeGreaterThan(0);

      // Should start with <svg and end with </svg>
      expect(svg.trim().startsWith('<svg')).toBe(true);
      expect(svg.trim().endsWith('</svg>')).toBe(true);
    });
  });

  describe('package inference', () => {
    it('should infer packages from context paths', () => {
      const manifest: Manifest = {
        version: '0.1.0',
        schema_version: '2.0.0',
        generated_at: '2026-01-25T00:00:00Z',
        bounded_contexts: [
          {
            name: 'orders',
            path: './packages/aef-domain/src/contexts/orders',
            features: []
          },
          {
            name: 'payments',
            path: './packages/aef-adapters/src/contexts/payments',
            features: []
          }
        ]
      };

      const generator = new ArchitectureSvgGenerator(manifest);
      const svg = generator.generate();

      // Should have inferred packages
      expect(svg).toContain('Packages');
      expect(svg).toContain('aef-domain');
      expect(svg).toContain('aef-adapters');
    });
  });

  describe('slice type grouping', () => {
    it('should group features by slice type', () => {
      const manifest: Manifest = {
        version: '0.1.0',
        schema_version: '2.2.0',
        generated_at: '2026-02-02T00:00:00Z',
        bounded_contexts: [
          {
            name: 'orchestration',
            path: './contexts/orchestration',
            context_type: 'bounded_context',
            aggregate_count: 3,
            features: [
              { name: 'create-workspace', path: 'slices/create-workspace', files: [], slice_type: 'command' },
              { name: 'execute-workflow', path: 'slices/execute-workflow', files: [], slice_type: 'command' },
              { name: 'workspace-metrics', path: 'slices/workspace-metrics', files: [], slice_type: 'query' },
              { name: 'session-stats', path: 'slices/session-stats', files: [], slice_type: 'query' }
            ]
          }
        ]
      };

      const generator = new ArchitectureSvgGenerator(manifest);
      const svg = generator.generate();

      // Should have slice type section headers
      expect(svg).toContain('⚡ Commands');
      expect(svg).toContain('📊 Queries');
      
      // Should list features under correct sections
      expect(svg).toContain('create-workspace');
      expect(svg).toContain('execute-workflow');
      expect(svg).toContain('workspace-metrics');
      expect(svg).toContain('session-stats');
    });

    it('should show mixed and unknown slice types', () => {
      const manifest: Manifest = {
        version: '0.1.0',
        schema_version: '2.2.0',
        generated_at: '2026-02-02T00:00:00Z',
        bounded_contexts: [
          {
            name: 'test-context',
            path: './contexts/test',
            features: [
              { name: 'mixed-feature', path: 'slices/mixed', files: [], slice_type: 'mixed' },
              { name: 'unknown-feature', path: 'slices/unknown', files: [], slice_type: 'unknown' },
              { name: 'no-type-feature', path: 'slices/no-type', files: [] }
            ]
          }
        ]
      };

      const generator = new ArchitectureSvgGenerator(manifest);
      const svg = generator.generate();

      expect(svg).toContain('🔀 Mixed');
      expect(svg).toContain('📁 Other');
    });
  });

  describe('aggregate count display', () => {
    it('should show aggregate count in context header', () => {
      const manifest: Manifest = {
        version: '0.1.0',
        schema_version: '2.2.0',
        generated_at: '2026-02-02T00:00:00Z',
        bounded_contexts: [
          {
            name: 'orchestration',
            path: './contexts/orchestration',
            context_type: 'bounded_context',
            aggregate_count: 3,
            features: []
          }
        ]
      };

      const generator = new ArchitectureSvgGenerator(manifest);
      const svg = generator.generate();

      expect(svg).toContain('3 aggregates');
    });

    it('should show singular "aggregate" for count of 1', () => {
      const manifest: Manifest = {
        version: '0.1.0',
        schema_version: '2.2.0',
        generated_at: '2026-02-02T00:00:00Z',
        bounded_contexts: [
          {
            name: 'github',
            path: './contexts/github',
            context_type: 'bounded_context',
            aggregate_count: 1,
            features: []
          }
        ]
      };

      const generator = new ArchitectureSvgGenerator(manifest);
      const svg = generator.generate();

      expect(svg).toContain('1 aggregate)');
      expect(svg).not.toContain('1 aggregates');
    });

    it('should handle missing aggregate_count (default to 0)', () => {
      const manifest: Manifest = {
        version: '0.1.0',
        schema_version: '2.2.0',
        generated_at: '2026-02-02T00:00:00Z',
        bounded_contexts: [
          {
            name: 'legacy-context',
            path: './contexts/legacy',
            features: []
          }
        ]
      };

      const generator = new ArchitectureSvgGenerator(manifest);
      const svg = generator.generate();

      expect(svg).toContain('0 aggregates');
    });
  });

  describe('context type filtering', () => {
    it('should exclude invalid_module contexts by default', () => {
      const manifest: Manifest = {
        version: '0.1.0',
        schema_version: '2.2.0',
        generated_at: '2026-02-02T00:00:00Z',
        bounded_contexts: [
          {
            name: 'orchestration',
            path: './contexts/orchestration',
            context_type: 'bounded_context',
            aggregate_count: 3,
            features: []
          },
          {
            name: 'metrics',
            path: './contexts/metrics',
            context_type: 'invalid_module',
            aggregate_count: 0,
            features: []
          }
        ]
      };

      const generator = new ArchitectureSvgGenerator(manifest);
      const svg = generator.generate();

      // Valid bounded context should be shown
      expect(svg).toContain('orchestration');
      // Invalid module should be filtered out
      expect(svg).not.toContain('>metrics<');
    });

    it('should include invalid_module contexts when showInvalidModules is true', () => {
      const manifest: Manifest = {
        version: '0.1.0',
        schema_version: '2.2.0',
        generated_at: '2026-02-02T00:00:00Z',
        bounded_contexts: [
          {
            name: 'orchestration',
            path: './contexts/orchestration',
            context_type: 'bounded_context',
            aggregate_count: 3,
            features: []
          },
          {
            name: 'metrics',
            path: './contexts/metrics',
            context_type: 'invalid_module',
            aggregate_count: 0,
            features: []
          }
        ]
      };

      const generator = new ArchitectureSvgGenerator(manifest, {}, { showInvalidModules: true });
      const svg = generator.generate();

      // Both should be shown
      expect(svg).toContain('orchestration');
      expect(svg).toContain('metrics');
    });

    it('should treat contexts without context_type as valid (backward compatibility)', () => {
      const manifest: Manifest = {
        version: '0.1.0',
        schema_version: '2.0.0', // Old schema without context_type
        generated_at: '2026-02-02T00:00:00Z',
        bounded_contexts: [
          {
            name: 'orders',
            path: './contexts/orders',
            features: []
          }
        ]
      };

      const generator = new ArchitectureSvgGenerator(manifest);
      const svg = generator.generate();

      // Should be shown (no context_type defaults to valid)
      expect(svg).toContain('orders');
    });
  });

  describe('dynamic height', () => {
    it('should calculate height based on content', () => {
      // Create manifest with many features that would exceed the old fixed height
      const manifest: Manifest = {
        version: '0.1.0',
        schema_version: '2.2.0',
        generated_at: '2026-02-02T00:00:00Z',
        bounded_contexts: [
          {
            name: 'orchestration',
            path: './contexts/orchestration',
            context_type: 'bounded_context',
            aggregate_count: 3,
            features: Array.from({ length: 15 }, (_, i) => ({
              name: `feature-${i + 1}`,
              path: `slices/feature-${i + 1}`,
              files: [],
              slice_type: 'command' as const
            }))
          }
        ]
      };

      const generator = new ArchitectureSvgGenerator(manifest);
      const svg = generator.generate();

      // All features should be present (no truncation)
      for (let i = 1; i <= 15; i++) {
        expect(svg).toContain(`feature-${i}`);
      }
      // Should NOT have "... and X more" since no truncation
      expect(svg).not.toContain('... and');
    });

    it('should respect maxFeaturesPerSection config', () => {
      const manifest: Manifest = {
        version: '0.1.0',
        schema_version: '2.2.0',
        generated_at: '2026-02-02T00:00:00Z',
        bounded_contexts: [
          {
            name: 'orchestration',
            path: './contexts/orchestration',
            context_type: 'bounded_context',
            aggregate_count: 3,
            features: Array.from({ length: 10 }, (_, i) => ({
              name: `command-${i + 1}`,
              path: `slices/command-${i + 1}`,
              files: [],
              slice_type: 'command' as const
            }))
          }
        ]
      };

      const generator = new ArchitectureSvgGenerator(manifest, {}, { maxFeaturesPerSection: 3 });
      const svg = generator.generate();

      // First 3 should be present
      expect(svg).toContain('command-1');
      expect(svg).toContain('command-2');
      expect(svg).toContain('command-3');
      // Should show "... and 7 more"
      expect(svg).toContain('... and 7 more');
    });
  });

  describe('CQRS layer', () => {
    it('should render CQRS layer when domain data is present', () => {
      const manifestWithDomain: Manifest = {
        version: '0.1.0',
        schema_version: '2.1.0',
        generated_at: '2026-01-26T00:00:00Z',
        bounded_contexts: [],
        domain: {
          aggregates: [],
          commands: [
            { name: 'CreateOrderCommand', file_path: 'commands/CreateOrder.ts', has_aggregate_id: true, fields: [] },
            { name: 'CancelOrderCommand', file_path: 'commands/CancelOrder.ts', has_aggregate_id: true, fields: [] }
          ],
          events: [
            { name: 'OrderCreatedEvent', event_type: 'OrderCreated', version: 'v1', file_path: 'events/OrderCreated.ts', emitted_by: [], handled_by: [], fields: [], decorator_present: true },
            { name: 'OrderCancelledEvent', event_type: 'OrderCancelled', version: 'v1', file_path: 'events/OrderCancelled.ts', emitted_by: [], handled_by: [], fields: [], decorator_present: true }
          ],
          projections: [
            { name: 'OrderListProjection', file_path: 'projections/OrderList.ts', subscribed_events: ['OrderCreatedEvent'], read_model: 'OrderSummary', line_count: 50 }
          ],
          relationships: {
            command_to_aggregate: {},
            aggregate_to_events: {},
            event_to_handlers: {}
          }
        }
      };

      const generator = new ArchitectureSvgGenerator(manifestWithDomain);
      const svg = generator.generate();

      // Check CQRS layer is present
      expect(svg).toContain('CQRS Pattern');
      expect(svg).toContain('Commands');
      expect(svg).toContain('Events');
      expect(svg).toContain('Projections');
      
      // Check counts
      expect(svg).toContain('2 total'); // Commands count
      expect(svg).toContain('1 total'); // Projections count
    });

    it('should not render CQRS layer when no domain data', () => {
      const generator = new ArchitectureSvgGenerator(minimalManifest);
      const svg = generator.generate();

      expect(svg).not.toContain('CQRS Pattern');
      expect(svg).not.toContain('Commands');
      expect(svg).not.toContain('Events');
      expect(svg).not.toContain('Projections');
    });

    it('should handle CQRS layer with zero projections', () => {
      const manifestWithoutProjections: Manifest = {
        version: '0.1.0',
        schema_version: '2.1.0',
        generated_at: '2026-01-26T00:00:00Z',
        bounded_contexts: [],
        domain: {
          aggregates: [],
          commands: [
            { name: 'CreateTaskCommand', file_path: 'commands/CreateTask.ts', has_aggregate_id: true, fields: [] }
          ],
          events: [
            { name: 'TaskCreatedEvent', event_type: 'TaskCreated', version: 'v1', file_path: 'events/TaskCreated.ts', emitted_by: [], handled_by: [], fields: [], decorator_present: true }
          ],
          projections: [],
          relationships: {
            command_to_aggregate: {},
            aggregate_to_events: {},
            event_to_handlers: {}
          }
        }
      };

      const generator = new ArchitectureSvgGenerator(manifestWithoutProjections);
      const svg = generator.generate();

      // CQRS layer should still render, but with 0 projections
      expect(svg).toContain('CQRS Pattern');
      expect(svg).toContain('Projections');
      expect(svg).toContain('0 total'); // Zero projections
    });

    it('should show correct CQRS colors for each box', () => {
      const manifestWithDomain: Manifest = {
        version: '0.1.0',
        schema_version: '2.1.0',
        generated_at: '2026-01-26T00:00:00Z',
        bounded_contexts: [],
        domain: {
          aggregates: [],
          commands: [{ name: 'TestCommand', file_path: 'commands/Test.ts', has_aggregate_id: true, fields: [] }],
          events: [{ name: 'TestEvent', event_type: 'Test', version: 'v1', file_path: 'events/Test.ts', emitted_by: [], handled_by: [], fields: [], decorator_present: true }],
          projections: [{ name: 'TestProjection', file_path: 'projections/Test.ts', subscribed_events: ['TestEvent'], line_count: 30 }],
          relationships: {
            command_to_aggregate: {},
            aggregate_to_events: {},
            event_to_handlers: {}
          }
        }
      };

      const generator = new ArchitectureSvgGenerator(manifestWithDomain);
      const svg = generator.generate();

      // Check for CQRS colors (command: blue, event: orange, projection: green)
      expect(svg).toContain('fill="#e3f2fd"'); // Command box (blue)
      expect(svg).toContain('stroke="#1976d2"');
      expect(svg).toContain('fill="#fff3e0"'); // Event box (orange)
      expect(svg).toContain('stroke="#f57c00"');
      expect(svg).toContain('fill="#e8f5e9"'); // Projection box (green)
      expect(svg).toContain('stroke="#388e3c"');
    });

    it('should include arrows between CQRS boxes', () => {
      const manifestWithDomain: Manifest = {
        version: '0.1.0',
        schema_version: '2.1.0',
        generated_at: '2026-01-26T00:00:00Z',
        bounded_contexts: [],
        domain: {
          aggregates: [],
          commands: [{ name: 'TestCommand', file_path: 'commands/Test.ts', has_aggregate_id: true, fields: [] }],
          events: [{ name: 'TestEvent', event_type: 'Test', version: 'v1', file_path: 'events/Test.ts', emitted_by: [], handled_by: [], fields: [], decorator_present: true }],
          projections: [{ name: 'TestProjection', file_path: 'projections/Test.ts', subscribed_events: ['TestEvent'], line_count: 30 }],
          relationships: {
            command_to_aggregate: {},
            aggregate_to_events: {},
            event_to_handlers: {}
          }
        }
      };

      const generator = new ArchitectureSvgGenerator(manifestWithDomain);
      const svg = generator.generate();

      // Check for arrow elements (lines and polygons for arrow heads)
      expect(svg).toContain('<line');
      expect(svg).toContain('<polygon'); // Arrow heads
      expect(svg).toContain('fill="#666666"'); // Arrow color
    });
  });
});
