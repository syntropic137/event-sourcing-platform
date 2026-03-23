import { BaseGenerator } from './base-generator';
import { Manifest } from '../types/manifest';

/**
 * Represents a flow of events across aggregates
 */
interface Flow {
  id: string;
  name: string;
  steps: FlowStep[];
}

/**
 * A step in a flow
 */
interface FlowStep {
  type: 'event' | 'command' | 'aggregate';
  name: string;
  source?: string;
  target?: string;
}

/**
 * Generates FLOWS.md documenting cross-aggregate event flows and sagas
 */
export class FlowsGenerator extends BaseGenerator {
  constructor(manifest: Manifest) {
    super(manifest);
  }

  protected getTitle(): string {
    return 'Cross-Aggregate Flows';
  }

  /**
   * Generate the flows document
   */
  generate(): string | null {
    const flows = this.detectFlows();

    if (flows.length === 0) {
      // No cross-aggregate flows detected
      return null;
    }

    let content = this.generateHeader();

    content += this.paragraph(
      'This document visualizes event flows that span multiple aggregates, ' +
        'including sagas and process managers that coordinate complex business workflows.'
    );

    // Statistics
    content += this.section('Flow Summary', 2);
    content += this.list([
      `**Total Flows**: ${flows.length}`,
      `**Aggregates Involved**: ${this.getUniqueAggregates(flows).size}`,
      `**Cross-Aggregate Events**: ${this.getCrossAggregateEvents().length}`,
    ]);

    // Generate flow diagrams
    content += this.section('Detected Flows', 2);
    for (const flow of flows) {
      content += this.section(flow.name, 3);
      content += this.generateFlowDiagram(flow);
    }

    // Event handler details
    content += this.section('Event Handler Details', 2);
    content += this.generateEventHandlerTable();

    return content;
  }

  /**
   * Detect cross-aggregate flows
   */
  private detectFlows(): Flow[] {
    const flows: Flow[] = [];
    const relationships = this.manifest.domain!.relationships;
    const processedEvents = new Set<string>();

    // Find events that are handled by aggregates other than the emitter
    for (const [eventName, handlers] of Object.entries(relationships.event_to_handlers)) {
      if (processedEvents.has(eventName)) continue;

      const event = this.manifest.domain!.events.find((e) => e.name === eventName);
      if (!event) continue;

      const emittedBy = event.emitted_by[0];

      // Find handlers in other aggregates
      const crossAggregateHandlers = handlers.filter((h) => h !== emittedBy);

      if (crossAggregateHandlers.length > 0) {
        // This is a cross-aggregate flow
        const flow: Flow = {
          id: this.sanitizeId(eventName),
          name: `${eventName} Flow`,
          steps: [
            {
              type: 'aggregate',
              name: emittedBy,
            },
            {
              type: 'event',
              name: eventName,
              source: emittedBy,
            },
          ],
        };

        // Add handler steps
        for (const handler of crossAggregateHandlers) {
          flow.steps.push({
            type: 'aggregate',
            name: handler,
            source: eventName,
          });

          // Check if this handler emits other events
          const handlerEvents = relationships.aggregate_to_events[handler] || [];
          for (const nextEvent of handlerEvents) {
            flow.steps.push({
              type: 'event',
              name: nextEvent,
              source: handler,
            });
          }
        }

        flows.push(flow);
        processedEvents.add(eventName);
      }
    }

    return flows;
  }

  /**
   * Collect unique aggregate participants from flow steps.
   */
  private collectParticipants(steps: Flow['steps']): string {
    const aggregates = new Set<string>();
    for (const step of steps) {
      if (step.type === 'aggregate') aggregates.add(step.name);
    }
    let lines = '';
    for (const agg of aggregates) {
      lines += `    participant ${this.sanitizeId(agg)} as ${agg}\n`;
    }
    return lines;
  }

  /**
   * Render flow steps as Mermaid sequence diagram interactions.
   */
  private renderFlowSteps(steps: Flow['steps']): string {
    let lines = '';
    let currentAggregate: string | null = null;

    for (let i = 0; i < steps.length; i++) {
      const step = steps[i];
      if (step.type === 'aggregate') {
        currentAggregate = step.name;
      } else if (step.type === 'event' && currentAggregate) {
        const sanitized = this.sanitizeId(currentAggregate);
        lines += `    ${sanitized}->>+${sanitized}: Process\n`;
        lines += `    ${sanitized}-->>-${sanitized}: ${step.name}\n`;

        const nextStep = steps[i + 1];
        if (nextStep && nextStep.type === 'aggregate' && nextStep.name !== currentAggregate) {
          lines += `    ${sanitized}-)${this.sanitizeId(nextStep.name)}: ${step.name}\n`;
        }
      }
    }
    return lines;
  }

  /**
   * Generate a sequence diagram for a flow
   */
  private generateFlowDiagram(flow: Flow): string {
    const diagram = `sequenceDiagram\n${this.collectParticipants(flow.steps)}\n${this.renderFlowSteps(flow.steps)}`;
    return this.wrapMermaid(diagram);
  }

  /**
   * Generate event handler details table
   */
  private generateEventHandlerTable(): string {
    const relationships = this.manifest.domain!.relationships;
    const rows: string[][] = [];

    for (const [eventName, handlers] of Object.entries(relationships.event_to_handlers)) {
      const event = this.manifest.domain!.events.find((e) => e.name === eventName);
      if (!event) continue;

      const emittedBy = event.emitted_by.join(', ');
      const handledBy = handlers.join(', ');
      const isCrossAggregate = handlers.some((h) => !event.emitted_by.includes(h));

      rows.push([`**${eventName}**`, emittedBy, handledBy, isCrossAggregate ? '✓' : '-']);
    }

    if (rows.length === 0) {
      return this.paragraph('*No event handlers detected.*');
    }

    return this.table(['Event', 'Emitted By', 'Handled By', 'Cross-Aggregate'], rows);
  }

  /**
   * Get all cross-aggregate events
   */
  private getCrossAggregateEvents(): string[] {
    const relationships = this.manifest.domain!.relationships;
    const crossAggregateEvents: string[] = [];

    for (const [eventName, handlers] of Object.entries(relationships.event_to_handlers)) {
      const event = this.manifest.domain!.events.find((e) => e.name === eventName);
      if (!event) continue;

      const isCrossAggregate = handlers.some((h) => !event.emitted_by.includes(h));
      if (isCrossAggregate) {
        crossAggregateEvents.push(eventName);
      }
    }

    return crossAggregateEvents;
  }

  /**
   * Get unique aggregates involved in flows
   */
  private getUniqueAggregates(flows: Flow[]): Set<string> {
    const aggregates = new Set<string>();

    for (const flow of flows) {
      for (const step of flow.steps) {
        if (step.type === 'aggregate') {
          aggregates.add(step.name);
        }
      }
    }

    return aggregates;
  }

  /**
   * Sanitize a name to be used as a Mermaid ID
   */
  private sanitizeId(name: string): string {
    return name.replace(/[^a-zA-Z0-9]/g, '_');
  }
}
