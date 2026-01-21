import { AggregateGenerator } from '../../src/generators/aggregate-generator';
import { Manifest } from '../../src/types/manifest';

describe('AggregateGenerator', () => {
  const createManifestWithAggregates = (): Manifest => ({
    version: '0.6.1-beta',
    schema_version: '1.0.0',
    generated_at: '2026-01-21T00:00:00Z',
    domain: {
      aggregates: [
        {
          name: 'OrderAggregate',
          file_path: 'src/domain/OrderAggregate.ts',
          line_count: 200,
          command_handlers: [
            {
              command_type: 'CreateOrder',
              method_name: 'handleCreateOrder',
              line_number: 20,
              emits_events: ['OrderCreated'],
            },
            {
              command_type: 'CancelOrder',
              method_name: 'handleCancelOrder',
              line_number: 40,
              emits_events: ['OrderCancelled'],
            },
          ],
          event_handlers: [
            {
              event_type: 'OrderCreated',
              method_name: 'onOrderCreated',
              line_number: 60,
            },
            {
              event_type: 'OrderCancelled',
              method_name: 'onOrderCancelled',
              line_number: 70,
            },
          ],
        },
        {
          name: 'PaymentAggregate',
          file_path: 'src/domain/PaymentAggregate.ts',
          line_count: 150,
          command_handlers: [],
          event_handlers: [],
        },
      ],
      commands: [
        {
          name: 'CreateOrder',
          file_path: 'src/commands/CreateOrder.ts',
          target_aggregate: 'OrderAggregate',
          has_aggregate_id: true,
          fields: [
            {
              name: 'orderId',
              field_type: 'string',
              required: true,
              line_number: 5,
            },
            {
              name: 'items',
              field_type: 'array',
              required: true,
              line_number: 6,
            },
          ],
        },
        {
          name: 'CancelOrder',
          file_path: 'src/commands/CancelOrder.ts',
          target_aggregate: 'OrderAggregate',
          has_aggregate_id: true,
          fields: [
            {
              name: 'orderId',
              field_type: 'string',
              required: true,
              line_number: 5,
            },
          ],
        },
      ],
      events: [
        {
          name: 'OrderCreated',
          event_type: 'order.created',
          version: '1.0.0',
          file_path: 'src/events/OrderCreated.ts',
          emitted_by: ['OrderAggregate'],
          handled_by: ['OrderAggregate'],
          fields: [
            {
              name: 'orderId',
              field_type: 'string',
              required: true,
              line_number: 8,
            },
            {
              name: 'items',
              field_type: 'array',
              required: true,
              line_number: 9,
            },
          ],
          decorator_present: true,
        },
        {
          name: 'OrderCancelled',
          event_type: 'order.cancelled',
          version: '1.0.0',
          file_path: 'src/events/OrderCancelled.ts',
          emitted_by: ['OrderAggregate'],
          handled_by: ['OrderAggregate'],
          fields: [
            {
              name: 'orderId',
              field_type: 'string',
              required: true,
              line_number: 8,
            },
          ],
          decorator_present: true,
        },
      ],
      relationships: {
        command_to_aggregate: {
          CreateOrder: 'OrderAggregate',
          CancelOrder: 'OrderAggregate',
        },
        aggregate_to_events: {
          OrderAggregate: ['OrderCreated', 'OrderCancelled'],
        },
        event_to_handlers: {
          OrderCreated: ['OrderAggregate'],
          OrderCancelled: ['OrderAggregate'],
        },
      },
    },
  });

  describe('generateAll', () => {
    it('should generate pages for all aggregates', () => {
      const manifest = createManifestWithAggregates();
      const generator = new AggregateGenerator(manifest);
      const pages = generator.generateAll();

      expect(pages.size).toBe(3); // 2 aggregates + 1 index
      expect(pages.has('aggregates/OrderAggregate.md')).toBe(true);
      expect(pages.has('aggregates/PaymentAggregate.md')).toBe(true);
      expect(pages.has('aggregates/README.md')).toBe(true);
    });

    it('should include metadata in aggregate pages', () => {
      const manifest = createManifestWithAggregates();
      const generator = new AggregateGenerator(manifest);
      const pages = generator.generateAll();

      const orderPage = pages.get('aggregates/OrderAggregate.md')!;
      expect(orderPage).toContain('# OrderAggregate');
      expect(orderPage).toContain('**File**: `src/domain/OrderAggregate.ts`');
      expect(orderPage).toContain('**Lines**: 200');
      expect(orderPage).toContain('**Commands**: 2');
      expect(orderPage).toContain('**Events**: 2');
    });

    it('should generate index page with links', () => {
      const manifest = createManifestWithAggregates();
      const generator = new AggregateGenerator(manifest);
      const pages = generator.generateAll();

      const indexPage = pages.get('aggregates/README.md')!;
      expect(indexPage).toContain('# Aggregates');
      expect(indexPage).toContain('[**OrderAggregate**](./OrderAggregate.md)');
      expect(indexPage).toContain('[**PaymentAggregate**](./PaymentAggregate.md)');
      expect(indexPage).toContain('2 commands, 2 events');
      expect(indexPage).toContain('0 commands, 0 events');
    });
  });

  describe('commands section', () => {
    it('should generate commands table', () => {
      const manifest = createManifestWithAggregates();
      const generator = new AggregateGenerator(manifest);
      const pages = generator.generateAll();

      const orderPage = pages.get('aggregates/OrderAggregate.md')!;
      expect(orderPage).toContain('## Commands Handled');
      expect(orderPage).toContain('| Command | Handler Method | Emits Events | Fields |');
      expect(orderPage).toContain('**CreateOrder**');
      expect(orderPage).toContain('`handleCreateOrder`');
      expect(orderPage).toContain('OrderCreated');
      expect(orderPage).toContain('2 fields');
    });

    it('should show multiple emitted events', () => {
      const manifest = createManifestWithAggregates();
      manifest.domain!.aggregates[0].command_handlers[0].emits_events = [
        'OrderCreated',
        'InventoryReserved',
      ];

      const generator = new AggregateGenerator(manifest);
      const pages = generator.generateAll();

      const orderPage = pages.get('aggregates/OrderAggregate.md')!;
      expect(orderPage).toContain('OrderCreated, InventoryReserved');
    });

    it('should handle commands with no events', () => {
      const manifest = createManifestWithAggregates();
      manifest.domain!.aggregates[0].command_handlers[0].emits_events = [];

      const generator = new AggregateGenerator(manifest);
      const pages = generator.generateAll();

      const orderPage = pages.get('aggregates/OrderAggregate.md')!;
      expect(orderPage).toContain('None');
    });

    it('should handle aggregate with no commands', () => {
      const manifest = createManifestWithAggregates();
      const generator = new AggregateGenerator(manifest);
      const pages = generator.generateAll();

      const paymentPage = pages.get('aggregates/PaymentAggregate.md')!;
      expect(paymentPage).toContain('*This aggregate does not handle any commands.*');
    });
  });

  describe('events section', () => {
    it('should generate events table', () => {
      const manifest = createManifestWithAggregates();
      const generator = new AggregateGenerator(manifest);
      const pages = generator.generateAll();

      const orderPage = pages.get('aggregates/OrderAggregate.md')!;
      expect(orderPage).toContain('## Events Handled');
      expect(orderPage).toContain('| Event | Handler Method | Version | Fields |');
      expect(orderPage).toContain('**OrderCreated**');
      expect(orderPage).toContain('`onOrderCreated`');
      expect(orderPage).toContain('1.0.0');
      expect(orderPage).toContain('2 fields');
    });

    it('should handle aggregate with no events', () => {
      const manifest = createManifestWithAggregates();
      const generator = new AggregateGenerator(manifest);
      const pages = generator.generateAll();

      const paymentPage = pages.get('aggregates/PaymentAggregate.md')!;
      expect(paymentPage).toContain('*This aggregate does not handle any events.*');
    });
  });

  describe('command flow diagram', () => {
    it('should generate sequence diagram for commands', () => {
      const manifest = createManifestWithAggregates();
      const generator = new AggregateGenerator(manifest);
      const pages = generator.generateAll();

      const orderPage = pages.get('aggregates/OrderAggregate.md')!;
      expect(orderPage).toContain('## Command Flow');
      expect(orderPage).toContain('```mermaid');
      expect(orderPage).toContain('sequenceDiagram');
      expect(orderPage).toContain('C->>A: CreateOrder');
      expect(orderPage).toContain('A->>E: OrderCreated');
      expect(orderPage).toContain('C->>A: CancelOrder');
      expect(orderPage).toContain('A->>E: OrderCancelled');
    });

    it('should not generate flow diagram for aggregate with no commands', () => {
      const manifest = createManifestWithAggregates();
      const generator = new AggregateGenerator(manifest);
      const pages = generator.generateAll();

      const paymentPage = pages.get('aggregates/PaymentAggregate.md')!;
      expect(paymentPage).not.toContain('## Command Flow');
    });
  });

  describe('state transitions diagram', () => {
    it('should generate state diagram for events', () => {
      const manifest = createManifestWithAggregates();
      const generator = new AggregateGenerator(manifest);
      const pages = generator.generateAll();

      const orderPage = pages.get('aggregates/OrderAggregate.md')!;
      expect(orderPage).toContain('## State Transitions');
      expect(orderPage).toContain('stateDiagram-v2');
      expect(orderPage).toContain('[*] --> Initial');
      expect(orderPage).toContain('Order Created');
      expect(orderPage).toContain('Order Cancelled');
    });

    it('should not generate state diagram for aggregate with no events', () => {
      const manifest = createManifestWithAggregates();
      const generator = new AggregateGenerator(manifest);
      const pages = generator.generateAll();

      const paymentPage = pages.get('aggregates/PaymentAggregate.md')!;
      expect(paymentPage).not.toContain('## State Transitions');
    });
  });

  describe('edge cases', () => {
    it('should handle single aggregate', () => {
      const manifest = createManifestWithAggregates();
      manifest.domain!.aggregates = [manifest.domain!.aggregates[0]];

      const generator = new AggregateGenerator(manifest);
      const pages = generator.generateAll();

      expect(pages.size).toBe(2); // 1 aggregate + 1 index
    });

    it('should handle aggregate names with special characters', () => {
      const manifest = createManifestWithAggregates();
      manifest.domain!.aggregates[0].name = 'Order-Aggregate.v2';

      const generator = new AggregateGenerator(manifest);
      const pages = generator.generateAll();

      const page = pages.get('aggregates/Order-Aggregate.v2.md')!;
      expect(page).toBeDefined();
      expect(page).toContain('# Order-Aggregate.v2');
    });

    it('should throw error when calling generate()', () => {
      const manifest = createManifestWithAggregates();
      const generator = new AggregateGenerator(manifest);

      expect(() => generator.generate()).toThrow('Use generateAll()');
    });
  });
});
