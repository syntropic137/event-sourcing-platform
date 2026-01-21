import { BaseGenerator } from './base-generator';
import { Manifest, Aggregate } from '../types/manifest';

/**
 * Generates detailed documentation for each aggregate
 */
export class AggregateGenerator extends BaseGenerator {
  constructor(manifest: Manifest) {
    super(manifest);
  }

  protected getTitle(): string {
    return 'Aggregates';
  }

  /**
   * Generate all aggregate pages
   * @returns Map of relative file paths to content
   */
  generateAll(): Map<string, string> {
    const pages = new Map<string, string>();
    const aggregates = this.manifest.domain!.aggregates;

    // Generate a page for each aggregate
    for (const aggregate of aggregates) {
      const content = this.generateAggregatePage(aggregate);
      pages.set(`aggregates/${aggregate.name}.md`, content);
    }

    // Generate index page
    const indexContent = this.generateIndexPage(aggregates);
    pages.set('aggregates/README.md', indexContent);

    return pages;
  }

  /**
   * Generate the main content (not used for aggregate generator)
   */
  generate(): string {
    throw new Error('Use generateAll() instead for AggregateGenerator');
  }

  /**
   * Generate index page linking to all aggregates
   */
  private generateIndexPage(aggregates: Aggregate[]): string {
    let content = `# Aggregates\n\n`;
    content += `> **Generated**: ${new Date().toISOString().split('T')[0]}  
> **VSA Version**: ${this.manifest.version}  
> **Schema Version**: ${this.manifest.schema_version}

---

`;

    content += this.paragraph(
      'This directory contains detailed documentation for each aggregate in the system. ' +
        'Aggregates are the consistency boundaries that handle commands and emit events.'
    );

    content += this.section('Aggregates in this System', 2);

    const aggregateLinks = aggregates.map((agg) => {
      const cmdCount = agg.command_handlers.length;
      const evtCount = agg.event_handlers.length;
      return `[**${agg.name}**](./${agg.name}.md) - ${cmdCount} commands, ${evtCount} events`;
    });

    content += this.list(aggregateLinks);

    return content;
  }

  /**
   * Generate detailed page for a single aggregate
   */
  private generateAggregatePage(aggregate: Aggregate): string {
    let content = `# ${aggregate.name}\n\n`;

    // Metadata
    content += `> **File**: \`${aggregate.file_path}\`  
> **Lines**: ${aggregate.line_count}  
> **Commands**: ${aggregate.command_handlers.length}  
> **Events**: ${aggregate.event_handlers.length}

---

`;

    content += this.paragraph(
      `The ${aggregate.name} is a domain aggregate that maintains consistency for its bounded context.`
    );

    // Commands Handled
    content += this.section('Commands Handled', 2);
    content += this.generateCommandsSection(aggregate);

    // Events Handled
    content += this.section('Events Handled', 2);
    content += this.generateEventsSection(aggregate);

    // Command Flow Diagram
    if (aggregate.command_handlers.length > 0) {
      content += this.section('Command Flow', 2);
      content += this.paragraph('Sequence diagram showing how commands are processed.');
      content += this.generateCommandFlowDiagram(aggregate);
    }

    // State Transitions
    if (aggregate.event_handlers.length > 0) {
      content += this.section('State Transitions', 2);
      content += this.paragraph('How events modify the aggregate state.');
      content += this.generateStateTransitionDiagram(aggregate);
    }

    return content;
  }

  /**
   * Generate commands section with details
   */
  private generateCommandsSection(aggregate: Aggregate): string {
    if (aggregate.command_handlers.length === 0) {
      return this.paragraph('*This aggregate does not handle any commands.*');
    }

    const commands = this.manifest.domain!.commands;
    const rows: string[][] = [];

    for (const handler of aggregate.command_handlers) {
      const command = commands.find((c) => c.name === handler.command_type);
      const emittedEvents = handler.emits_events.join(', ') || 'None';

      rows.push([
        `**${handler.command_type}**`,
        `\`${handler.method_name}\``,
        emittedEvents,
        command ? `${command.fields.length} fields` : 'N/A',
      ]);
    }

    return this.table(['Command', 'Handler Method', 'Emits Events', 'Fields'], rows);
  }

  /**
   * Generate events section with details
   */
  private generateEventsSection(aggregate: Aggregate): string {
    if (aggregate.event_handlers.length === 0) {
      return this.paragraph('*This aggregate does not handle any events.*');
    }

    const events = this.manifest.domain!.events;

    const rows: string[][] = [];

    for (const handler of aggregate.event_handlers) {
      const event = events.find((e) => e.name === handler.event_type);

      rows.push([
        `**${handler.event_type}**`,
        `\`${handler.method_name}\``,
        event ? event.version : 'N/A',
        event ? `${event.fields.length} fields` : 'N/A',
      ]);
    }

    return this.table(['Event', 'Handler Method', 'Version', 'Fields'], rows);
  }

  /**
   * Generate command flow sequence diagram
   */
  private generateCommandFlowDiagram(aggregate: Aggregate): string {
    let diagram = `sequenceDiagram
    participant C as Command Handler
    participant A as ${aggregate.name}
    participant E as Event Store

`;

    for (const handler of aggregate.command_handlers) {
      diagram += `    C->>A: ${handler.command_type}\n`;
      diagram += `    activate A\n`;

      if (handler.emits_events.length > 0) {
        for (const event of handler.emits_events) {
          diagram += `    A->>E: ${event}\n`;
        }
      }

      diagram += `    A-->>C: Success\n`;
      diagram += `    deactivate A\n\n`;
    }

    return this.wrapMermaid(diagram);
  }

  /**
   * Generate state transition diagram
   */
  private generateStateTransitionDiagram(aggregate: Aggregate): string {
    let diagram = `stateDiagram-v2
    [*] --> Initial

`;

    // Group events by type
    const uniqueEvents = new Set(aggregate.event_handlers.map((h) => h.event_type));

    for (const event of uniqueEvents) {
      const stateName = event.replace(/([A-Z])/g, ' $1').trim();
      diagram += `    Initial --> ${this.sanitizeId(event)}: ${stateName}\n`;
    }

    return this.wrapMermaid(diagram);
  }

  /**
   * Sanitize a name to be used as a Mermaid ID
   */
  private sanitizeId(name: string): string {
    return name.replace(/[^a-zA-Z0-9]/g, '_');
  }
}
