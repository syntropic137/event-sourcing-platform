import { AggregateRelationshipGenerator } from '../../src/generators/aggregate-relationship-generator';
import { Manifest } from '../../src/types/manifest';

describe('AggregateRelationshipGenerator', () => {
  describe('generate', () => {
    it('should generate empty state when no aggregates', () => {
      const manifest: Manifest = {
        version: '0.7.0',
        schema_version: '2.3.0',
        generated_at: '2026-02-02T00:00:00Z',
        bounded_contexts: [],
      };

      const generator = new AggregateRelationshipGenerator(manifest);
      const svg = generator.generate();

      expect(svg).toContain('<svg');
      expect(svg).toContain('No Aggregates Found');
      expect(svg).toContain('Use --include-domain flag');
    });

    it('should generate SVG with aggregate boxes', () => {
      const manifest: Manifest = {
        version: '0.7.0',
        schema_version: '2.3.0',
        generated_at: '2026-02-02T00:00:00Z',
        domain: {
          aggregates: [
            {
              name: 'WorkspaceAggregate',
              file_path: 'domain/aggregate_workspace/WorkspaceAggregate.py',
              line_count: 200,
              command_handlers: [],
              event_handlers: [],
              context: 'orchestration',
              folder_name: 'aggregate_workspace',
              entities: [],
              value_objects: [],
            },
          ],
          commands: [],
          events: [],
          relationships: {
            command_to_aggregate: {},
            aggregate_to_events: {},
            event_to_handlers: {},
          },
        },
      };

      const generator = new AggregateRelationshipGenerator(manifest);
      const svg = generator.generate();

      expect(svg).toContain('<svg');
      expect(svg).toContain('Workspace'); // Name without Aggregate suffix
      expect(svg).toContain('orchestration');
      expect(svg).toContain('aggregate_workspace');
    });

    it('should show entities within aggregate', () => {
      const manifest: Manifest = {
        version: '0.7.0',
        schema_version: '2.3.0',
        generated_at: '2026-02-02T00:00:00Z',
        domain: {
          aggregates: [
            {
              name: 'WorkspaceAggregate',
              file_path: 'domain/aggregate_workspace/WorkspaceAggregate.py',
              line_count: 200,
              command_handlers: [],
              event_handlers: [],
              context: 'orchestration',
              entities: [
                {
                  name: 'IsolationHandle',
                  identity_field: 'isolation_id',
                  file_path: 'domain/aggregate_workspace/IsolationHandle.py',
                  line_count: 50,
                },
              ],
              value_objects: [],
            },
          ],
          commands: [],
          events: [],
          relationships: {
            command_to_aggregate: {},
            aggregate_to_events: {},
            event_to_handlers: {},
          },
        },
      };

      const generator = new AggregateRelationshipGenerator(manifest);
      const svg = generator.generate();

      expect(svg).toContain('Entities:');
      expect(svg).toContain('IsolationHandle');
      expect(svg).toContain('isolation_id');
    });

    it('should show value objects within aggregate', () => {
      const manifest: Manifest = {
        version: '0.7.0',
        schema_version: '2.3.0',
        generated_at: '2026-02-02T00:00:00Z',
        domain: {
          aggregates: [
            {
              name: 'WorkspaceAggregate',
              file_path: 'domain/aggregate_workspace/WorkspaceAggregate.py',
              line_count: 200,
              command_handlers: [],
              event_handlers: [],
              context: 'orchestration',
              entities: [],
              value_objects: [
                {
                  name: 'SecurityPolicy',
                  file_path: 'domain/aggregate_workspace/SecurityPolicy.py',
                  is_immutable: true,
                  line_count: 30,
                },
                {
                  name: 'ExecutionResult',
                  file_path: 'domain/aggregate_workspace/ExecutionResult.py',
                  is_immutable: false,
                  line_count: 25,
                },
              ],
            },
          ],
          commands: [],
          events: [],
          relationships: {
            command_to_aggregate: {},
            aggregate_to_events: {},
            event_to_handlers: {},
          },
        },
      };

      const generator = new AggregateRelationshipGenerator(manifest);
      const svg = generator.generate();

      expect(svg).toContain('Value Objects:');
      expect(svg).toContain('SecurityPolicy');
      expect(svg).toContain('ExecutionResult');
      // Immutable indicator
      expect(svg).toContain('❄️');
    });

    it('should show event subscriptions', () => {
      const manifest: Manifest = {
        version: '0.7.0',
        schema_version: '2.3.0',
        generated_at: '2026-02-02T00:00:00Z',
        domain: {
          aggregates: [
            {
              name: 'WorkflowExecutionAggregate',
              file_path: 'domain/aggregate_execution/WorkflowExecutionAggregate.py',
              line_count: 300,
              command_handlers: [],
              event_handlers: [
                {
                  event_type: 'WorkspaceCreated',
                  method_name: 'on_workspace_created',
                  line_number: 50,
                },
                {
                  event_type: 'WorkflowStarted',
                  method_name: 'on_workflow_started',
                  line_number: 70,
                },
              ],
              context: 'orchestration',
              entities: [],
              value_objects: [],
            },
          ],
          commands: [],
          events: [],
          relationships: {
            command_to_aggregate: {},
            aggregate_to_events: {},
            event_to_handlers: {},
          },
        },
      };

      const generator = new AggregateRelationshipGenerator(manifest);
      const svg = generator.generate();

      expect(svg).toContain('Subscribes to:');
      expect(svg).toContain('WorkspaceCreated');
      expect(svg).toContain('WorkflowStarted');
    });

    it('should group aggregates by context', () => {
      const manifest: Manifest = {
        version: '0.7.0',
        schema_version: '2.3.0',
        generated_at: '2026-02-02T00:00:00Z',
        domain: {
          aggregates: [
            {
              name: 'WorkspaceAggregate',
              file_path: 'domain/WorkspaceAggregate.py',
              line_count: 200,
              command_handlers: [],
              event_handlers: [],
              context: 'orchestration',
            },
            {
              name: 'AgentSessionAggregate',
              file_path: 'domain/AgentSessionAggregate.py',
              line_count: 150,
              command_handlers: [],
              event_handlers: [],
              context: 'sessions',
            },
          ],
          commands: [],
          events: [],
          relationships: {
            command_to_aggregate: {},
            aggregate_to_events: {},
            event_to_handlers: {},
          },
        },
      };

      const generator = new AggregateRelationshipGenerator(manifest);
      const svg = generator.generate();

      expect(svg).toContain('orchestration');
      expect(svg).toContain('sessions');
      expect(svg).toContain('Workspace');
      expect(svg).toContain('AgentSession');
    });

    it('should filter by context when specified', () => {
      const manifest: Manifest = {
        version: '0.7.0',
        schema_version: '2.3.0',
        generated_at: '2026-02-02T00:00:00Z',
        domain: {
          aggregates: [
            {
              name: 'WorkspaceAggregate',
              file_path: 'domain/WorkspaceAggregate.py',
              line_count: 200,
              command_handlers: [],
              event_handlers: [],
              context: 'orchestration',
            },
            {
              name: 'AgentSessionAggregate',
              file_path: 'domain/AgentSessionAggregate.py',
              line_count: 150,
              command_handlers: [],
              event_handlers: [],
              context: 'sessions',
            },
          ],
          commands: [],
          events: [],
          relationships: {
            command_to_aggregate: {},
            aggregate_to_events: {},
            event_to_handlers: {},
          },
        },
      };

      const generator = new AggregateRelationshipGenerator(manifest, {
        contextFilter: 'orchestration',
      });
      const svg = generator.generate();

      expect(svg).toContain('orchestration');
      expect(svg).toContain('Workspace');
      expect(svg).not.toContain('sessions');
      expect(svg).not.toContain('AgentSession');
    });

    it('should hide details when showDetails is false', () => {
      const manifest: Manifest = {
        version: '0.7.0',
        schema_version: '2.3.0',
        generated_at: '2026-02-02T00:00:00Z',
        domain: {
          aggregates: [
            {
              name: 'WorkspaceAggregate',
              file_path: 'domain/WorkspaceAggregate.py',
              line_count: 200,
              command_handlers: [],
              event_handlers: [],
              context: 'orchestration',
              entities: [
                {
                  name: 'IsolationHandle',
                  identity_field: 'isolation_id',
                  file_path: 'domain/IsolationHandle.py',
                  line_count: 50,
                },
              ],
              value_objects: [],
            },
          ],
          commands: [],
          events: [],
          relationships: {
            command_to_aggregate: {},
            aggregate_to_events: {},
            event_to_handlers: {},
          },
        },
      };

      const generator = new AggregateRelationshipGenerator(manifest, {
        showDetails: false,
      });
      const svg = generator.generate();

      expect(svg).toContain('Workspace');
      expect(svg).not.toContain('Entities:');
      expect(svg).not.toContain('IsolationHandle');
    });

    it('should render event flow arrows between aggregates', () => {
      const manifest: Manifest = {
        version: '0.7.0',
        schema_version: '2.3.0',
        generated_at: '2026-02-02T00:00:00Z',
        domain: {
          aggregates: [
            {
              name: 'WorkspaceAggregate',
              file_path: 'domain/WorkspaceAggregate.py',
              line_count: 200,
              command_handlers: [
                {
                  command_type: 'CreateWorkspace',
                  method_name: 'handle',
                  line_number: 10,
                  emits_events: ['WorkspaceCreated'],
                },
              ],
              event_handlers: [],
              context: 'orchestration',
            },
            {
              name: 'WorkflowExecutionAggregate',
              file_path: 'domain/WorkflowExecutionAggregate.py',
              line_count: 300,
              command_handlers: [],
              event_handlers: [
                {
                  event_type: 'WorkspaceCreated',
                  method_name: 'on_workspace_created',
                  line_number: 50,
                },
              ],
              context: 'orchestration',
            },
          ],
          commands: [],
          events: [],
          relationships: {
            command_to_aggregate: {},
            aggregate_to_events: {},
            event_to_handlers: {},
          },
        },
      };

      const generator = new AggregateRelationshipGenerator(manifest, {
        showEventFlow: true,
      });
      const svg = generator.generate();

      // Should have a line element for the arrow
      expect(svg).toContain('<line');
      // Should show the event type as label
      expect(svg).toContain('WorkspaceCreated');
    });

    it('should not render event flow when showEventFlow is false', () => {
      const manifest: Manifest = {
        version: '0.7.0',
        schema_version: '2.3.0',
        generated_at: '2026-02-02T00:00:00Z',
        domain: {
          aggregates: [
            {
              name: 'WorkspaceAggregate',
              file_path: 'domain/WorkspaceAggregate.py',
              line_count: 200,
              command_handlers: [
                {
                  command_type: 'CreateWorkspace',
                  method_name: 'handle',
                  line_number: 10,
                  emits_events: ['WorkspaceCreated'],
                },
              ],
              event_handlers: [],
              context: 'orchestration',
            },
            {
              name: 'WorkflowExecutionAggregate',
              file_path: 'domain/WorkflowExecutionAggregate.py',
              line_count: 300,
              command_handlers: [],
              event_handlers: [
                {
                  event_type: 'WorkspaceCreated',
                  method_name: 'on_workspace_created',
                  line_number: 50,
                },
              ],
              context: 'orchestration',
            },
          ],
          commands: [],
          events: [],
          relationships: {
            command_to_aggregate: {},
            aggregate_to_events: {},
            event_to_handlers: {},
          },
        },
      };

      const generator = new AggregateRelationshipGenerator(manifest, {
        showEventFlow: false,
      });
      const svg = generator.generate();

      // Should not have arrow lines between aggregates
      // (The only line should be in the legend if any)
      expect(svg).not.toContain('→ Event Flow');
    });

    it('should render legend with component explanations', () => {
      const manifest: Manifest = {
        version: '0.7.0',
        schema_version: '2.3.0',
        generated_at: '2026-02-02T00:00:00Z',
        domain: {
          aggregates: [
            {
              name: 'WorkspaceAggregate',
              file_path: 'domain/WorkspaceAggregate.py',
              line_count: 200,
              command_handlers: [],
              event_handlers: [],
              context: 'orchestration',
            },
          ],
          commands: [],
          events: [],
          relationships: {
            command_to_aggregate: {},
            aggregate_to_events: {},
            event_to_handlers: {},
          },
        },
      };

      const generator = new AggregateRelationshipGenerator(manifest);
      const svg = generator.generate();

      expect(svg).toContain('Legend:');
      expect(svg).toContain('Aggregate Root');
      expect(svg).toContain('Entity');
      expect(svg).toContain('Value Object');
    });

    it('should render title', () => {
      const manifest: Manifest = {
        version: '0.7.0',
        schema_version: '2.3.0',
        generated_at: '2026-02-02T00:00:00Z',
        domain: {
          aggregates: [
            {
              name: 'WorkspaceAggregate',
              file_path: 'domain/WorkspaceAggregate.py',
              line_count: 200,
              command_handlers: [],
              event_handlers: [],
            },
          ],
          commands: [],
          events: [],
          relationships: {
            command_to_aggregate: {},
            aggregate_to_events: {},
            event_to_handlers: {},
          },
        },
      };

      const generator = new AggregateRelationshipGenerator(manifest);
      const svg = generator.generate();

      expect(svg).toContain('Aggregate Relationships');
    });
  });
});
