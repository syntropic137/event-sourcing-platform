import { OverviewGenerator } from '../../src/generators/overview-generator';
import { Manifest } from '../../src/types/manifest';

describe('OverviewGenerator', () => {
  const createBasicManifest = (): Manifest => ({
    version: '0.6.1-beta',
    schema_version: '1.0.0',
    generated_at: '2026-01-21T00:00:00Z',
    domain: {
      aggregates: [
        {
          name: 'TestAggregate',
          file_path: 'src/domain/TestAggregate.ts',
          line_count: 100,
          command_handlers: [
            {
              command_type: 'CreateTest',
              method_name: 'handleCreate',
              line_number: 10,
              emits_events: ['TestCreated'],
            },
          ],
          event_handlers: [
            {
              event_type: 'TestCreated',
              method_name: 'onTestCreated',
              line_number: 20,
            },
          ],
        },
      ],
      commands: [
        {
          name: 'CreateTest',
          file_path: 'src/commands/CreateTest.ts',
          target_aggregate: 'TestAggregate',
          has_aggregate_id: true,
          fields: [],
        },
      ],
      events: [
        {
          name: 'TestCreated',
          event_type: 'test.created',
          version: '1.0.0',
          file_path: 'src/events/TestCreated.ts',
          emitted_by: ['TestAggregate'],
          handled_by: ['TestAggregate'],
          fields: [],
          decorator_present: true,
        },
      ],
      relationships: {
        command_to_aggregate: {
          CreateTest: 'TestAggregate',
        },
        aggregate_to_events: {
          TestAggregate: ['TestCreated'],
        },
        event_to_handlers: {
          TestCreated: ['TestAggregate'],
        },
      },
    },
  });

  describe('generate', () => {
    it('should generate overview for single-context system', () => {
      const manifest = createBasicManifest();
      const generator = new OverviewGenerator(manifest);
      const output = generator.generate();

      expect(output).toContain('# System Overview');
      expect(output).toContain('## Statistics');
      expect(output).toContain('**Aggregates**: 1');
      expect(output).toContain('**Commands**: 1');
      expect(output).toContain('**Events**: 1');
    });

    it('should include generated date and version', () => {
      const manifest = createBasicManifest();
      const generator = new OverviewGenerator(manifest);
      const output = generator.generate();

      expect(output).toContain('**Generated**:');
      expect(output).toContain('**VSA Version**: 0.6.1-beta');
      expect(output).toContain('**Schema Version**: 1.0.0');
    });

    it('should generate C4 context diagram', () => {
      const manifest = createBasicManifest();
      const generator = new OverviewGenerator(manifest);
      const output = generator.generate();

      expect(output).toContain('## System Context');
      expect(output).toContain('```mermaid');
      expect(output).toContain('C4Context');
      expect(output).toContain('Person(user,');
      expect(output).toContain('System(system,');
    });

    it('should generate aggregates overview', () => {
      const manifest = createBasicManifest();
      const generator = new OverviewGenerator(manifest);
      const output = generator.generate();

      expect(output).toContain('## Aggregates');
      expect(output).toContain('TestAggregate');
      expect(output).toContain('1 commands, 1 events');
    });

    it('should generate event flow diagram', () => {
      const manifest = createBasicManifest();
      const generator = new OverviewGenerator(manifest);
      const output = generator.generate();

      expect(output).toContain('## Event Flow');
      expect(output).toContain('graph LR');
      expect(output).toContain('TestAggregate');
      expect(output).toContain('TestCreated');
    });
  });

  describe('multi-context system', () => {
    it('should generate bounded contexts section', () => {
      const manifest = createBasicManifest();
      manifest.bounded_contexts = [
        {
          name: 'Orders',
          description: 'Order management',
          aggregates: ['OrderAggregate'],
          publishes: ['OrderPlaced'],
          subscribes: [],
        },
        {
          name: 'Shipping',
          description: 'Shipping management',
          aggregates: ['ShipmentAggregate'],
          publishes: ['ShipmentCreated'],
          subscribes: ['OrderPlaced'],
        },
      ];

      const generator = new OverviewGenerator(manifest);
      const output = generator.generate();

      expect(output).toContain('## Bounded Contexts');
      expect(output).toContain('C4Container');
      expect(output).toContain('Orders');
      expect(output).toContain('Shipping');
    });

    it('should show bounded context count in statistics', () => {
      const manifest = createBasicManifest();
      manifest.bounded_contexts = [
        {
          name: 'Context1',
          aggregates: [],
          publishes: [],
          subscribes: [],
        },
        {
          name: 'Context2',
          aggregates: [],
          publishes: [],
          subscribes: [],
        },
      ];

      const generator = new OverviewGenerator(manifest);
      const output = generator.generate();

      expect(output).toContain('**Bounded Contexts**: 2');
    });

    it('should generate context details section', () => {
      const manifest = createBasicManifest();
      manifest.bounded_contexts = [
        {
          name: 'Orders',
          description: 'Order management',
          aggregates: ['OrderAggregate'],
          publishes: ['OrderPlaced'],
          subscribes: [],
        },
      ];

      const generator = new OverviewGenerator(manifest);
      const output = generator.generate();

      expect(output).toContain('### Orders');
      expect(output).toContain('Order management');
      expect(output).toContain('**Aggregates**: OrderAggregate');
      expect(output).toContain('**Publishes**: OrderPlaced');
    });

    it('should generate integration events diagram', () => {
      const manifest = createBasicManifest();
      manifest.bounded_contexts = [
        {
          name: 'Orders',
          aggregates: ['OrderAggregate'],
          publishes: ['OrderPlaced'],
          subscribes: [],
        },
        {
          name: 'Shipping',
          aggregates: ['ShipmentAggregate'],
          publishes: [],
          subscribes: ['OrderPlaced'],
        },
      ];

      const generator = new OverviewGenerator(manifest);
      const output = generator.generate();

      expect(output).toContain('## Integration Events');
      expect(output).toContain('Orders');
      expect(output).toContain('Shipping');
      expect(output).toContain('OrderPlaced');
    });
  });

  describe('optional fields', () => {
    it('should include queries in statistics when present', () => {
      const manifest = createBasicManifest();
      manifest.domain!.queries = [
        {
          name: 'GetTest',
          file_path: 'src/queries/GetTest.ts',
          fields: [],
        },
      ];

      const generator = new OverviewGenerator(manifest);
      const output = generator.generate();

      expect(output).toContain('**Queries**: 1');
    });

    it('should include upcasters in statistics when present', () => {
      const manifest = createBasicManifest();
      manifest.domain!.upcasters = [
        {
          name: 'TestCreatedV1ToV2',
          file_path: 'src/upcasters/TestCreatedV1ToV2.ts',
          from_version: '1.0.0',
          to_version: '2.0.0',
          event_type: 'TestCreated',
        },
      ];

      const generator = new OverviewGenerator(manifest);
      const output = generator.generate();

      expect(output).toContain('**Upcasters**: 1');
    });

    it('should not show queries section when not present', () => {
      const manifest = createBasicManifest();
      manifest.domain!.queries = undefined;

      const generator = new OverviewGenerator(manifest);
      const output = generator.generate();

      expect(output).not.toContain('**Queries**');
    });
  });

  describe('edge cases', () => {
    it('should handle system with no events', () => {
      const manifest = createBasicManifest();
      manifest.domain!.events = [];
      manifest.domain!.relationships.aggregate_to_events = {};
      manifest.domain!.relationships.event_to_handlers = {};

      const generator = new OverviewGenerator(manifest);
      const output = generator.generate();

      expect(output).not.toContain('## Event Flow');
    });

    it('should handle multiple aggregates', () => {
      const manifest = createBasicManifest();
      manifest.domain!.aggregates.push({
        name: 'AnotherAggregate',
        file_path: 'src/domain/AnotherAggregate.ts',
        line_count: 50,
        command_handlers: [],
        event_handlers: [],
      });

      const generator = new OverviewGenerator(manifest);
      const output = generator.generate();

      expect(output).toContain('TestAggregate');
      expect(output).toContain('AnotherAggregate');
      expect(output).toContain('**Aggregates**: 2');
    });

    it('should sanitize IDs for Mermaid diagrams', () => {
      const manifest = createBasicManifest();
      manifest.domain!.aggregates[0].name = 'Test-Aggregate.v2';

      const generator = new OverviewGenerator(manifest);
      const output = generator.generate();

      // Should convert special characters to underscores
      expect(output).toContain('Test_Aggregate_v2');
    });
  });
});
