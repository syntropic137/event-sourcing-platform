//! TypeScript templates

/// Command template
pub const COMMAND_TEMPLATE: &str = r#"{{#if framework}}import { {{framework.domain_event_class}} } from '{{framework.domain_event_import}}';
{{/if}}
/**
 * Command to {{feature_name}}
 */
export class {{command_name}} {
  constructor(
{{#each fields}}    public readonly {{name}}: {{field_type}},
{{/each}}  ) {}
}
"#;

/// Event template
pub const EVENT_TEMPLATE: &str = r#"{{#if framework}}import { {{framework.domain_event_class}} } from '{{framework.domain_event_import}}';

{{/if}}/**
 * Event representing {{feature_name}} completion
 */
export class {{event_name}}{{#if framework}} extends {{framework.domain_event_class}}{{/if}} {
  constructor(
{{#each fields}}    public readonly {{name}}: {{field_type}},
{{/each}}  ) {
{{#if framework}}    super();
{{/if}}  }
}
"#;

/// Handler template
pub const HANDLER_TEMPLATE: &str = r#"import { {{command_name}} } from './{{command_name}}';
import { {{event_name}} } from './{{event_name}}';
{{#if aggregate_name}}import { {{aggregate_name}} } from './{{aggregate_name}}';
{{/if}}{{#if framework}}{{#if framework.handler_import}}import { {{framework.handler_class}} } from '{{framework.handler_import}}';
{{/if}}{{/if}}
/**
 * Handler for {{command_name}}
 *
 * This handler processes the command, applies business logic,
 * creates events, and persists them to the event store.
 */
export class {{handler_name}}{{#if framework}}{{#if framework.handler_class}} extends {{framework.handler_class}}{{/if}}{{/if}} {
  constructor(private eventStore: any) {} // TODO: Type this properly with IEventStore interface

  async handle(command: {{command_name}}): Promise<void> {
    // TODO: Add validation logic

    // Create event
    const event = new {{event_name}}(
{{#each fields}}      command.{{name}},
{{/each}}    );
{{#if aggregate_name}}

    // Create aggregate and apply event
    const aggregate = new {{aggregate_name}}();
    aggregate.apply{{event_name}}(event);

    // TODO: Persist to event store
    // await this.eventStore.save(command.id, [event]);
{{else}}

    // TODO: Persist to event store
    // await this.eventStore.save(aggregateId, [event]);
{{/if}}  }
}
"#;

/// Test template
pub const TEST_TEMPLATE: &str = r#"import { describe, it, expect } from '@jest/globals';
import { {{command_name}} } from './{{command_name}}';
import { {{event_name}} } from './{{event_name}}';
import { {{handler_name}} } from './{{handler_name}}';

describe('{{test_name}}', () => {
  describe('{{command_name}}', () => {
    it('should create command with required fields', () => {
      const command = new {{command_name}}(
{{#each fields}}        'test-{{name}}',
{{/each}}      );

{{#each fields}}      expect(command.{{name}}).toBe('test-{{name}}');
{{/each}}    });
  });

  describe('{{event_name}}', () => {
    it('should create event with required fields', () => {
      const event = new {{event_name}}(
{{#each fields}}        'test-{{name}}',
{{/each}}      );

{{#each fields}}      expect(event.{{name}}).toBe('test-{{name}}');
{{/each}}    });
  });

  describe('{{handler_name}}', () => {
    it('should handle command and return event', async () => {
      const handler = new {{handler_name}}();
      const command = new {{command_name}}(
{{#each fields}}        'test-{{name}}',
{{/each}}      );

      const event = await handler.handle(command);

      expect(event).toBeInstanceOf({{event_name}});
{{#each fields}}      expect(event.{{name}}).toBe('test-{{name}}');
{{/each}}    });
  });
});
"#;

/// Aggregate template
pub const AGGREGATE_TEMPLATE: &str = r#"{{#if framework}}import { {{framework.aggregate_class}} } from '{{framework.aggregate_import}}';
{{/if}}import { {{event_name}} } from './{{event_name}}';

/**
 * Aggregate for {{feature_name}}
 *
 * AggregateRoot automatically routes events to their corresponding
 * @EventSourcingHandler methods based on event type.
 */
export class {{aggregate_name}}{{#if framework}} extends {{framework.aggregate_class}}{{/if}} {
{{#each fields}}  private {{name}}: {{field_type}}{{#unless is_required}} | null{{/unless}};
{{/each}}
  constructor() {
{{#if framework}}    super();
{{/if}}{{#each fields}}    this.{{name}} = {{#if is_required}}'default'{{else}}null{{/if}};
{{/each}}  }

  /**
   * Apply {{event_name}}
   *
   * This method is automatically called when a {{event_name}}
   * is applied to the aggregate via the @EventSourcingHandler decorator.
   */
  apply{{event_name}}(event: {{event_name}}): void {
{{#each fields}}    this.{{name}} = event.{{name}};
{{/each}}  }

{{#each fields}}  get{{name_pascal}}(): {{field_type}}{{#unless is_required}} | null{{/unless}} {
    return this.{{name}};
  }
{{/each}}}
"#;

// =============================================================================
// QUERY SLICE TEMPLATES (CQRS Read Side)
// =============================================================================

/// Query template (DTO for query parameters)
pub const QUERY_TEMPLATE: &str = r#"/**
 * Query to {{feature_name}}
 *
 * This query DTO defines the parameters for the read operation.
 */
export class {{query_name}} {
  constructor(
{{#each fields}}    public readonly {{name}}: {{field_type}},
{{/each}}  ) {}
}
"#;

/// Projection template (builds read model from events)
pub const PROJECTION_TEMPLATE: &str = r#"import { Projection, Handles } from '@event-sourcing/core';
{{#each subscribed_events}}import { {{this}} } from '../../domain/events/{{this}}';
{{/each}}

/**
 * Projection for {{feature_name}}
 *
 * This projection builds the {{read_model_name}} read model by processing
 * domain events. The read model is optimized for the specific query needs.
 */
export class {{projection_name}} extends Projection<{{read_model_name}}> {
  private items: Map<string, {{read_model_name}}> = new Map();

{{#each subscribed_events}}
  @Handles({{this}})
  async on{{this}}(event: {{this}}): Promise<void> {
    // TODO: Update read model based on event data
    // const item = this.items.get(event.aggregateId) || this.createDefault();
    // this.items.set(event.aggregateId, updatedItem);
  }
{{/each}}

  /**
   * Get all items in the read model
   */
  getAll(): {{read_model_name}}[] {
    return Array.from(this.items.values());
  }

  /**
   * Get a specific item by ID
   */
  getById(id: string): {{read_model_name}} | undefined {
    return this.items.get(id);
  }

  /**
   * Create a default read model item
   */
  private createDefault(): {{read_model_name}} {
    // TODO: Return default read model structure
    return {} as {{read_model_name}};
  }
}

/**
 * Read model for {{feature_name}}
 */
export interface {{read_model_name}} {
{{#each fields}}  {{name}}: {{field_type}};
{{/each}}
}
"#;

/// Query handler template (executes query against projection)
pub const QUERY_HANDLER_TEMPLATE: &str = r#"import { {{query_name}} } from './{{query_name}}';
import { {{projection_name}}, {{read_model_name}} } from './{{projection_name}}';

/**
 * Handler for {{query_name}}
 *
 * This handler executes the query against the projection's read model.
 * Handlers are thin - they simply retrieve data from the projection.
 */
export class {{query_handler_name}} {
  constructor(private readonly projection: {{projection_name}}) {}

  /**
   * Execute the query and return results
   */
  async handle(query: {{query_name}}): Promise<{{read_model_name}}{{#if is_list}}[]{{/if}}> {
{{#if is_list}}
    // Return all items from the projection
    return this.projection.getAll();
{{else}}
    // Return specific item by ID
    const result = this.projection.getById(query.id);
    if (!result) {
      throw new Error(`{{read_model_name}} not found`);
    }
    return result;
{{/if}}
  }
}
"#;

/// Query controller template (thin adapter for HTTP/REST)
pub const QUERY_CONTROLLER_TEMPLATE: &str = r#"import { QueryBus } from '@infrastructure/QueryBus';
import { {{query_name}} } from './{{query_name}}';

/**
 * Controller for {{feature_name}}
 *
 * This is a thin adapter that translates HTTP requests to query bus calls.
 * Controllers should NOT contain business logic - only request/response mapping.
 */
export class {{controller_name}} {
  constructor(private readonly queryBus: QueryBus) {}

  /**
   * Handle GET request
   */
  async handle(request: Request): Promise<Response> {
    const query = new {{query_name}}(
{{#each fields}}      request.params.{{name}},
{{/each}}    );

    const result = await this.queryBus.execute(query);

    return new Response(JSON.stringify(result), {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
    });
  }
}
"#;

/// Query test template
pub const QUERY_TEST_TEMPLATE: &str = r#"import { describe, it, expect, beforeEach } from '@jest/globals';
import { {{query_name}} } from './{{query_name}}';
import { {{projection_name}} } from './{{projection_name}}';
import { {{query_handler_name}} } from './{{query_handler_name}}';

describe('{{feature_name}}', () => {
  let projection: {{projection_name}};
  let handler: {{query_handler_name}};

  beforeEach(() => {
    projection = new {{projection_name}}();
    handler = new {{query_handler_name}}(projection);
  });

  describe('{{query_name}}', () => {
    it('should create query with required fields', () => {
      const query = new {{query_name}}(
{{#each fields}}        'test-{{name}}',
{{/each}}      );

{{#each fields}}      expect(query.{{name}}).toBe('test-{{name}}');
{{/each}}    });
  });

  describe('{{projection_name}}', () => {
    it('should start with empty read model', () => {
      const items = projection.getAll();
      expect(items).toHaveLength(0);
    });

    // TODO: Add event handling tests
  });

  describe('{{query_handler_name}}', () => {
    it('should execute query against projection', async () => {
      const query = new {{query_name}}(
{{#each fields}}        'test-{{name}}',
{{/each}}      );

      // TODO: Setup projection with test data first
      // const result = await handler.handle(query);
      // expect(result).toBeDefined();
    });
  });
});
"#;

/// Slice manifest template (slice.yaml)
pub const SLICE_MANIFEST_TEMPLATE: &str = r#"name: {{feature_name}}
type: {{slice_type}}
{{#if projection_name}}projection: {{projection_name}}
{{/if}}{{#if subscribed_events}}subscribes_to:
{{#each subscribed_events}}  - {{this}}
{{/each}}{{/if}}{{#if read_model_name}}returns: {{read_model_name}}
{{/if}}description: |
  {{#if is_list}}Lists {{feature_name}} from the read model.{{else}}Gets {{feature_name}} details from the read model.{{/if}}
"#;
