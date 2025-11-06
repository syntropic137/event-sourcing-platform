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
