import { BaseGenerator } from './base-generator';
import { Manifest } from '../types/manifest';
import { getContextCount } from '../manifest/parser';

/**
 * Generates OVERVIEW.md with system-level diagrams
 */
export class OverviewGenerator extends BaseGenerator {
  constructor(manifest: Manifest) {
    super(manifest);
  }

  protected getTitle(): string {
    return 'System Overview';
  }

  /**
   * Generate the complete overview document
   */
  generate(): string {
    const contextCount = getContextCount(this.manifest);
    let content = this.generateHeader();

    // Add description
    content += this.paragraph(
      'This document provides a high-level overview of the system architecture, ' +
        'including bounded contexts, aggregates, and their relationships.'
    );

    // Generate statistics
    content += this.generateStatistics();

    // Generate appropriate diagrams based on context count
    if (contextCount === 0) {
      content += this.generateSimpleOverview();
    } else {
      content += this.generateMultiContextOverview();
    }

    return content;
  }

  /**
   * Generate overview for single-context (or no explicit contexts)
   */
  private generateSimpleOverview(): string {
    let content = '';

    // C4 Context Diagram
    content += this.section('System Context', 2);
    content += this.paragraph(
      'High-level view of the system and its interactions with external actors.'
    );
    content += this.generateC4Context();

    // Aggregates Overview
    content += this.section('Aggregates', 2);
    content += this.paragraph(
      'Domain aggregates that encapsulate business logic and maintain consistency boundaries.'
    );
    content += this.generateAggregatesOverview();

    // Event Flow
    if (this.hasEvents()) {
      content += this.section('Event Flow', 2);
      content += this.paragraph('Overview of domain events and their relationships.');
      content += this.generateEventFlow();
    }

    return content;
  }

  /**
   * Generate overview for multi-context systems
   */
  private generateMultiContextOverview(): string {
    let content = '';

    // C4 Context Diagram
    content += this.section('System Context', 2);
    content += this.paragraph(
      'High-level view of the system showing bounded contexts and external actors.'
    );
    content += this.generateC4Context();

    // C4 Container Diagram (Bounded Contexts)
    content += this.section('Bounded Contexts', 2);
    content += this.paragraph(
      'The system is organized into multiple bounded contexts, each with its own domain model.'
    );
    content += this.generateC4Container();

    // Context Details
    content += this.section('Context Details', 2);
    content += this.generateContextDetails();

    // Integration Events
    if (this.hasIntegrationEvents()) {
      content += this.section('Integration Events', 2);
      content += this.paragraph('Events published and consumed across bounded context boundaries.');
      content += this.generateIntegrationEvents();
    }

    return content;
  }

  /**
   * Generate statistics summary
   */
  private generateStatistics(): string {
    const domain = this.manifest.domain!;
    const stats = [
      `**Aggregates**: ${domain.aggregates.length}`,
      `**Commands**: ${domain.commands.length}`,
      `**Events**: ${domain.events.length}`,
    ];

    if (domain.queries && domain.queries.length > 0) {
      stats.push(`**Queries**: ${domain.queries.length}`);
    }

    if (domain.upcasters && domain.upcasters.length > 0) {
      stats.push(`**Upcasters**: ${domain.upcasters.length}`);
    }

    if (this.manifest.bounded_contexts && this.manifest.bounded_contexts.length > 0) {
      stats.push(`**Bounded Contexts**: ${this.manifest.bounded_contexts.length}`);
    }

    let content = this.section('Statistics', 2);
    content += this.list(stats);
    return content;
  }

  /**
   * Generate C4 Context diagram
   */
  private generateC4Context(): string {
    const systemName = this.getSystemName();

    let diagram = `C4Context
  title System Context Diagram

  Person(user, "User", "Interacts with the system")
  System(system, "${systemName}", "Event-sourced system built with VSA")
`;

    // Add external systems if any (placeholder for future)
    diagram += `
  Rel(user, system, "Uses")
`;

    return this.wrapMermaid(diagram);
  }

  /**
   * Generate C4 Container diagram showing bounded contexts
   */
  private generateC4Container(): string {
    const systemName = this.getSystemName();
    const contexts = this.manifest.bounded_contexts!;

    let diagram = `C4Container
  title Container Diagram - Bounded Contexts

  Person(user, "User")
  System_Boundary(system, "${systemName}") {
`;

    // Add each bounded context as a container
    for (const context of contexts) {
      const contextId = this.sanitizeId(context.name);
      const aggCount = context.aggregates.length;
      diagram += `    Container(${contextId}, "${context.name} Context", "Bounded Context", "${aggCount} aggregate(s)")\n`;
    }

    diagram += `  }\n\n`;

    // Add relationships
    diagram += `  Rel(user, ${this.sanitizeId(contexts[0].name)}, "Uses")\n`;

    // Add integration event relationships
    for (const context of contexts) {
      if (context.publishes && context.publishes.length > 0) {
        for (const subscriber of contexts) {
          if (subscriber.name === context.name) continue;

          const hasSubscription = subscriber.subscribes?.some((event) =>
            context.publishes.includes(event)
          );

          if (hasSubscription) {
            diagram += `  Rel(${this.sanitizeId(context.name)}, ${this.sanitizeId(subscriber.name)}, "Publishes events")\n`;
          }
        }
      }
    }

    return this.wrapMermaid(diagram);
  }

  /**
   * Generate details for each bounded context
   */
  private generateContextDetails(): string {
    const contexts = this.manifest.bounded_contexts!;
    let content = '';

    for (const context of contexts) {
      content += this.section(context.name, 3);

      if (context.description) {
        content += this.paragraph(context.description);
      }

      const details = [
        `**Aggregates**: ${context.aggregates.join(', ') || 'None'}`,
        `**Publishes**: ${context.publishes?.join(', ') || 'None'}`,
        `**Subscribes**: ${context.subscribes?.join(', ') || 'None'}`,
      ];

      content += this.list(details);
    }

    return content;
  }

  /**
   * Generate aggregates overview diagram
   */
  private generateAggregatesOverview(): string {
    const aggregates = this.manifest.domain!.aggregates;
    const relationships = this.manifest.domain!.relationships;

    let diagram = `graph TB
`;

    // Add aggregates
    for (const aggregate of aggregates) {
      const aggId = this.sanitizeId(aggregate.name);
      const cmdCount = aggregate.command_handlers.length;
      const evtCount = aggregate.event_handlers.length;
      diagram += `    ${aggId}["${aggregate.name}<br/>${cmdCount} commands, ${evtCount} events"]\n`;
    }

    // Add command relationships
    for (const [command, aggregate] of Object.entries(relationships.command_to_aggregate)) {
      const cmdId = this.sanitizeId(command);
      const aggId = this.sanitizeId(aggregate);
      diagram += `    ${cmdId}((${command})) --> ${aggId}\n`;
    }

    return this.wrapMermaid(diagram);
  }

  /**
   * Generate event flow diagram
   */
  private generateEventFlow(): string {
    const relationships = this.manifest.domain!.relationships;

    let diagram = `graph LR
`;

    // Show aggregates and their events
    for (const [aggregate, eventNames] of Object.entries(relationships.aggregate_to_events)) {
      const aggId = this.sanitizeId(aggregate);

      for (const eventName of eventNames) {
        const evtId = this.sanitizeId(eventName);
        diagram += `    ${aggId}[${aggregate}] -->|emits| ${evtId}((${eventName}))\n`;

        // Show event handlers
        const handlers = relationships.event_to_handlers[eventName] || [];
        for (const handler of handlers) {
          if (handler !== aggregate) {
            const handlerId = this.sanitizeId(handler);
            diagram += `    ${evtId} -->|handled by| ${handlerId}[${handler}]\n`;
          }
        }
      }
    }

    return this.wrapMermaid(diagram);
  }

  /**
   * Generate integration events diagram
   */
  private generateIntegrationEvents(): string {
    const contexts = this.manifest.bounded_contexts!;

    let diagram = `graph TB
`;

    for (const context of contexts) {
      const contextId = this.sanitizeId(context.name);

      // Show published events
      if (context.publishes) {
        for (const event of context.publishes) {
          const eventId = this.sanitizeId(event);
          diagram += `    ${contextId}[${context.name}] -->|publishes| ${eventId}((${event}))\n`;

          // Find subscribers
          for (const subscriber of contexts) {
            if (subscriber.name !== context.name && subscriber.subscribes?.includes(event)) {
              const subscriberId = this.sanitizeId(subscriber.name);
              diagram += `    ${eventId} -->|subscribes| ${subscriberId}[${subscriber.name}]\n`;
            }
          }
        }
      }
    }

    return this.wrapMermaid(diagram);
  }

  /**
   * Check if system has events
   */
  private hasEvents(): boolean {
    return this.manifest.domain!.events.length > 0;
  }

  /**
   * Check if system has integration events
   */
  private hasIntegrationEvents(): boolean {
    if (!this.manifest.bounded_contexts) return false;

    return this.manifest.bounded_contexts.some(
      (ctx) =>
        (ctx.publishes && ctx.publishes.length > 0) || (ctx.subscribes && ctx.subscribes.length > 0)
    );
  }

  /**
   * Get a friendly system name
   */
  private getSystemName(): string {
    // Try to infer from bounded contexts or use a default
    if (this.manifest.bounded_contexts && this.manifest.bounded_contexts.length > 0) {
      return 'VSA System';
    }
    return 'VSA Application';
  }

  /**
   * Sanitize a name to be used as a Mermaid ID
   */
  private sanitizeId(name: string): string {
    return name.replace(/[^a-zA-Z0-9]/g, '_');
  }
}
