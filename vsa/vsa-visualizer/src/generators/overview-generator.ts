import { BaseGenerator } from './base-generator';
import { Manifest } from '../types/manifest';
import { getContextCount } from '../manifest/parser';

/**
 * Generates OVERVIEW.md with system-level diagrams
 */
export class OverviewGenerator extends BaseGenerator {
  /** Maximum number of features to display per context in overview */
  private readonly MAX_DISPLAYED_FEATURES = 10;

  /** Folder names to exclude when listing features (infrastructure/organizational folders) */
  private readonly EXCLUDED_FEATURE_FOLDERS = [
    'domain',
    'slices',
    'ports',
    'application',
    'events',
    '__pycache__',
  ];

  constructor(manifest: Manifest) {
    super(manifest);
  }

  protected getTitle(): string {
    return 'System Overview';
  }

  /**
   * Filter features to exclude infrastructure and organizational folders
   * @param features - Array of features to filter
   * @param infrastructureFolders - Optional array of context-specific infrastructure folders
   * @returns Filtered array of features
   */
  private filterFeatureFolders(
    features: Array<{ name: string; path: string; files: string[] }>,
    infrastructureFolders?: string[]
  ): Array<{ name: string; path: string; files: string[] }> {
    return features.filter((f) => {
      const isInfrastructure = infrastructureFolders?.includes(f.name);
      return (
        !isInfrastructure &&
        !this.EXCLUDED_FEATURE_FOLDERS.includes(f.name) &&
        !f.name.endsWith('__pycache__')
      );
    });
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

    // Domain Layer
    if (this.manifest.domain) {
      content += this.section('Domain Layer', 2);
      content += this.paragraph(
        'The domain layer contains pure business logic with no external dependencies.'
      );
      content += this.generateDomainLayerOverview();
    }

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
    content += this.paragraph(
      'Detailed breakdown of each bounded context and its features/slices.'
    );
    content += this.generateContextDetails();

    return content;
  }

  /**
   * Generate statistics summary
   */
  private generateStatistics(): string {
    const stats = [];

    if (this.manifest.bounded_contexts && this.manifest.bounded_contexts.length > 0) {
      stats.push(`**Bounded Contexts**: ${this.manifest.bounded_contexts.length}`);

      // Count total features across all contexts
      const totalFeatures = this.manifest.bounded_contexts.reduce(
        (sum, ctx) => sum + ctx.features.length,
        0
      );
      stats.push(`**Total Features/Slices**: ${totalFeatures}`);
    }

    if (this.manifest.domain) {
      const domain = this.manifest.domain;
      stats.push(`**Aggregates**: ${domain.aggregates.length}`);
      stats.push(`**Commands**: ${domain.commands.length}`);
      stats.push(`**Events**: ${domain.events.length}`);

      if (domain.queries && domain.queries.length > 0) {
        stats.push(`**Queries**: ${domain.queries.length}`);
      }

      if (domain.value_objects && domain.value_objects.length > 0) {
        stats.push(`**Value Objects**: ${domain.value_objects.length}`);
      }

      if (domain.upcasters && domain.upcasters.length > 0) {
        stats.push(`**Upcasters**: ${domain.upcasters.length}`);
      }
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
      const featureCount = context.features.length;
      diagram += `    Container(${contextId}, "${context.name}", "Bounded Context", "${featureCount} feature(s)")\n`;
    }

    diagram += `  }\n\n`;

    // Add relationships
    diagram += `  Rel(user, ${this.sanitizeId(contexts[0].name)}, "Uses")\n`;

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
      content += this.paragraph(`Path: \`${context.path}\``);

      // Show infrastructure folders (NEW in v2.0.0)
      if (context.infrastructure_folders && context.infrastructure_folders.length > 0) {
        content += this.paragraph('**Infrastructure:**');
        const infraList = context.infrastructure_folders.map((f) => `\`${f}\``);
        content += this.list(infraList);
      }

      // List features/slices (filter out infrastructure)
      const allFilteredFeatures = this.filterFeatureFolders(
        context.features,
        context.infrastructure_folders
      );
      const features = allFilteredFeatures.slice(0, this.MAX_DISPLAYED_FEATURES);

      if (features.length > 0) {
        content += this.paragraph('**Features/Slices:**');
        const featureList = features.map((f) => {
          const fileCount = f.files.length;
          return `\`${f.name}\` (${fileCount} files)`;
        });
        content += this.list(featureList);

        const totalFeatures = allFilteredFeatures.length;

        if (totalFeatures > features.length) {
          content += this.paragraph(`_... and ${totalFeatures - features.length} more features_`);
        }
      }
    }

    return content;
  }

  /**
   * Generate aggregates overview diagram
   */
  private generateAggregatesOverview(): string {
    if (!this.manifest.domain) {
      return this.paragraph('*No domain data available*');
    }

    const aggregates = this.manifest.domain.aggregates;
    const relationships = this.manifest.domain.relationships;

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
    if (!this.manifest.domain) {
      return this.paragraph('*No domain data available*');
    }

    const relationships = this.manifest.domain.relationships;

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
   * Generate domain layer overview (NEW in v2.0.0)
   */
  private generateDomainLayerOverview(): string {
    if (!this.manifest.domain) {
      return '';
    }

    const domain = this.manifest.domain;
    let content = '';

    // Show aggregates prominently
    if (domain.aggregates.length > 0) {
      content += this.section('Aggregates', 3);
      const aggList = domain.aggregates.map((agg) => {
        const cmdCount = agg.command_handlers.length;
        const evtCount = agg.event_handlers.length;
        return `**${agg.name}** - ${cmdCount} command handler(s), ${evtCount} event handler(s)`;
      });
      content += this.list(aggList);
    }

    // Show value objects (NEW in v2.0.0)
    if (domain.value_objects && domain.value_objects.length > 0) {
      content += this.section('Value Objects', 3);
      const voList = domain.value_objects.map((vo) => {
        const immutableBadge = vo.is_immutable ? ' 🔒' : '';
        return `**${vo.name}**${immutableBadge} - ${vo.line_count} lines`;
      });
      content += this.list(voList);
    }

    return content;
  }

  /**
   * Check if system has events
   */
  private hasEvents(): boolean {
    return this.manifest.domain !== undefined && this.manifest.domain.events.length > 0;
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
