import { FlowsGenerator } from '../../src/generators/flows-generator';
import { Manifest } from '../../src/types/manifest';

describe('FlowsGenerator', () => {
  const createManifestWithFlows = (): Manifest => ({
    version: '0.6.1-beta',
    schema_version: '1.0.0',
    generated_at: '2026-01-21T00:00:00Z',
    domain: {
      aggregates: [
        {
          name: 'OrderAggregate',
          file_path: 'src/domain/OrderAggregate.ts',
          line_count: 200,
          command_handlers: [],
          event_handlers: [],
        },
        {
          name: 'ShippingAggregate',
          file_path: 'src/domain/ShippingAggregate.ts',
          line_count: 150,
          command_handlers: [],
          event_handlers: [
            {
              event_type: 'OrderPlaced',
              method_name: 'onOrderPlaced',
              line_number: 20,
            },
          ],
        },
        {
          name: 'PaymentAggregate',
          file_path: 'src/domain/PaymentAggregate.ts',
          line_count: 180,
          command_handlers: [],
          event_handlers: [
            {
              event_type: 'OrderPlaced',
              method_name: 'onOrderPlaced',
              line_number: 30,
            },
          ],
        },
      ],
      commands: [],
      events: [
        {
          name: 'OrderPlaced',
          event_type: 'order.placed',
          version: '1.0.0',
          file_path: 'src/events/OrderPlaced.ts',
          emitted_by: ['OrderAggregate'],
          handled_by: ['OrderAggregate', 'ShippingAggregate', 'PaymentAggregate'],
          fields: [],
          decorator_present: true,
        },
        {
          name: 'ShipmentCreated',
          event_type: 'shipment.created',
          version: '1.0.0',
          file_path: 'src/events/ShipmentCreated.ts',
          emitted_by: ['ShippingAggregate'],
          handled_by: ['ShippingAggregate'],
          fields: [],
          decorator_present: true,
        },
        {
          name: 'PaymentProcessed',
          event_type: 'payment.processed',
          version: '1.0.0',
          file_path: 'src/events/PaymentProcessed.ts',
          emitted_by: ['PaymentAggregate'],
          handled_by: ['PaymentAggregate'],
          fields: [],
          decorator_present: true,
        },
      ],
      relationships: {
        command_to_aggregate: {},
        aggregate_to_events: {
          OrderAggregate: ['OrderPlaced'],
          ShippingAggregate: ['ShipmentCreated'],
          PaymentAggregate: ['PaymentProcessed'],
        },
        event_to_handlers: {
          OrderPlaced: ['OrderAggregate', 'ShippingAggregate', 'PaymentAggregate'],
          ShipmentCreated: ['ShippingAggregate'],
          PaymentProcessed: ['PaymentAggregate'],
        },
      },
    },
  });

  const createManifestWithoutFlows = (): Manifest => ({
    version: '0.6.1-beta',
    schema_version: '1.0.0',
    generated_at: '2026-01-21T00:00:00Z',
    domain: {
      aggregates: [
        {
          name: 'OrderAggregate',
          file_path: 'src/domain/OrderAggregate.ts',
          line_count: 200,
          command_handlers: [],
          event_handlers: [],
        },
      ],
      commands: [],
      events: [
        {
          name: 'OrderCreated',
          event_type: 'order.created',
          version: '1.0.0',
          file_path: 'src/events/OrderCreated.ts',
          emitted_by: ['OrderAggregate'],
          handled_by: ['OrderAggregate'],
          fields: [],
          decorator_present: true,
        },
      ],
      relationships: {
        command_to_aggregate: {},
        aggregate_to_events: {
          OrderAggregate: ['OrderCreated'],
        },
        event_to_handlers: {
          OrderCreated: ['OrderAggregate'],
        },
      },
    },
  });

  describe('generate', () => {
    it('should return null when no cross-aggregate flows exist', () => {
      const manifest = createManifestWithoutFlows();
      const generator = new FlowsGenerator(manifest);
      const output = generator.generate();

      expect(output).toBeNull();
    });

    it('should generate flows document when cross-aggregate flows exist', () => {
      const manifest = createManifestWithFlows();
      const generator = new FlowsGenerator(manifest);
      const output = generator.generate();

      expect(output).not.toBeNull();
      expect(output).toContain('# Cross-Aggregate Flows');
      expect(output).toContain('## Flow Summary');
      expect(output).toContain('## Detected Flows');
    });

    it('should include metadata in header', () => {
      const manifest = createManifestWithFlows();
      const generator = new FlowsGenerator(manifest);
      const output = generator.generate();

      expect(output).toContain('**Generated**:');
      expect(output).toContain('**VSA Version**: 0.6.1-beta');
      expect(output).toContain('**Schema Version**: 1.0.0');
    });
  });

  describe('flow detection', () => {
    it('should detect cross-aggregate event flows', () => {
      const manifest = createManifestWithFlows();
      const generator = new FlowsGenerator(manifest);
      const output = generator.generate();

      expect(output).toContain('OrderPlaced Flow');
      expect(output).toContain('ShippingAggregate');
      expect(output).toContain('PaymentAggregate');
    });

    it('should not detect same-aggregate flows', () => {
      const manifest = createManifestWithFlows();
      const generator = new FlowsGenerator(manifest);
      const output = generator.generate();

      // ShipmentCreated is only handled by ShippingAggregate (same as emitter)
      expect(output).not.toContain('ShipmentCreated Flow');
    });

    it('should include flow statistics', () => {
      const manifest = createManifestWithFlows();
      const generator = new FlowsGenerator(manifest);
      const output = generator.generate();

      expect(output).toContain('**Total Flows**:');
      expect(output).toContain('**Aggregates Involved**:');
      expect(output).toContain('**Cross-Aggregate Events**:');
    });
  });

  describe('flow diagrams', () => {
    it('should generate sequence diagrams for flows', () => {
      const manifest = createManifestWithFlows();
      const generator = new FlowsGenerator(manifest);
      const output = generator.generate();

      expect(output).toContain('```mermaid');
      expect(output).toContain('sequenceDiagram');
      expect(output).toContain('participant');
    });

    it('should include all participants in sequence diagram', () => {
      const manifest = createManifestWithFlows();
      const generator = new FlowsGenerator(manifest);
      const output = generator.generate();

      expect(output).toContain('OrderAggregate');
      expect(output).toContain('ShippingAggregate');
      expect(output).toContain('PaymentAggregate');
    });

    it('should show event emission and handling', () => {
      const manifest = createManifestWithFlows();
      const generator = new FlowsGenerator(manifest);
      const output = generator.generate();

      expect(output).toContain('OrderPlaced');
      expect(output).toContain('Process');
    });
  });

  describe('event handler table', () => {
    it('should generate event handler details table', () => {
      const manifest = createManifestWithFlows();
      const generator = new FlowsGenerator(manifest);
      const output = generator.generate();

      expect(output).toContain('## Event Handler Details');
      expect(output).toContain('| Event | Emitted By | Handled By | Cross-Aggregate |');
      expect(output).toContain('**OrderPlaced**');
      expect(output).toContain('OrderAggregate');
    });

    it('should mark cross-aggregate events', () => {
      const manifest = createManifestWithFlows();
      const generator = new FlowsGenerator(manifest);
      const output = generator.generate();

      // OrderPlaced is cross-aggregate
      const lines = output!.split('\n');
      const orderPlacedLine = lines.find((line) => line.includes('**OrderPlaced**'));
      expect(orderPlacedLine).toContain('✓');
    });

    it('should not mark same-aggregate events', () => {
      const manifest = createManifestWithFlows();
      const generator = new FlowsGenerator(manifest);
      const output = generator.generate();

      // ShipmentCreated is same-aggregate
      const lines = output!.split('\n');
      const shipmentLine = lines.find((line) => line.includes('**ShipmentCreated**'));
      expect(shipmentLine).toContain('-');
    });
  });

  describe('edge cases', () => {
    it('should handle multiple handlers for same event', () => {
      const manifest = createManifestWithFlows();
      const generator = new FlowsGenerator(manifest);
      const output = generator.generate();

      // OrderPlaced has 3 handlers total (1 emitter + 2 cross-aggregate)
      expect(output).toContain('ShippingAggregate');
      expect(output).toContain('PaymentAggregate');
    });

    it('should handle event with no fields', () => {
      const manifest = createManifestWithFlows();
      const generator = new FlowsGenerator(manifest);
      const output = generator.generate();

      expect(output).not.toBeNull();
      expect(output).toContain('OrderPlaced');
    });

    it('should sanitize IDs in diagrams', () => {
      const manifest = createManifestWithFlows();
      manifest.domain!.aggregates[0].name = 'Order-Aggregate.v2';
      manifest.domain!.events[0].emitted_by = ['Order-Aggregate.v2'];

      const generator = new FlowsGenerator(manifest);
      const output = generator.generate();

      // Should convert special characters to underscores
      expect(output).toContain('Order_Aggregate_v2');
    });
  });
});
