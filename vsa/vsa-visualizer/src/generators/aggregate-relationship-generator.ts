/**
 * Aggregate Relationship SVG Generator
 *
 * Generates SVG visualizations showing aggregate structure and relationships:
 * - Aggregate roots with their entities and value objects
 * - Event flow between aggregates (emits → subscribes)
 * - Grouping by bounded context
 *
 * NEW in v2.3.0
 */

import {
  Manifest,
  Aggregate,
  AggregateEntity,
  AggregateValueObject,
} from '../types/manifest';
import { SvgBuilder, Point } from '../utils/svg-builder';

/** Configuration for aggregate visualization */
export interface AggregateVisualizeConfig {
  /** Show entity/VO details within each aggregate box */
  showDetails?: boolean;
  /** Show event arrows between aggregates */
  showEventFlow?: boolean;
  /** Filter to specific context (null = all contexts) */
  contextFilter?: string | null;
  /** Maximum aggregates per row */
  aggregatesPerRow?: number;
}

/** Colors for aggregate components */
const AggregateColors = {
  root: '#d6c9ff',        // Pastel purple - aggregate root
  entity: '#c9d9ff',      // Pastel blue - entity
  valueObject: '#ffd9e8', // Pastel pink - value object
  eventArrow: '#666666',  // Arrow color
  subscribes: '#2196F3',  // Blue for subscription arrows
  emits: '#4CAF50',       // Green for emit arrows
  border: '#a0a0a0',
  text: {
    primary: '#1a1a1a',
    secondary: '#666666',
    tertiary: '#999999',
  },
};

/** Position and dimensions for an aggregate box */
interface AggregateLayout {
  aggregate: Aggregate;
  x: number;
  y: number;
  width: number;
  height: number;
}

/**
 * Generator for aggregate relationship SVG diagrams
 */
export class AggregateRelationshipGenerator {
  // Layout constants
  private readonly AGGREGATE_WIDTH = 220;
  private readonly AGGREGATE_MIN_HEIGHT = 100;
  private readonly AGGREGATE_PADDING = 15;
  private readonly AGGREGATE_SPACING_X = 60;
  private readonly AGGREGATE_SPACING_Y = 80;
  private readonly ENTITY_HEIGHT = 20;
  private readonly HEADER_HEIGHT = 50;
  private readonly CONTEXT_PADDING = 20;
  private readonly CANVAS_PADDING = 40;

  private config: AggregateVisualizeConfig;

  constructor(
    private manifest: Manifest,
    config: Partial<AggregateVisualizeConfig> = {}
  ) {
    this.config = {
      showDetails: true,
      showEventFlow: true,
      contextFilter: null,
      aggregatesPerRow: 3,
      ...config,
    };
  }

  /**
   * Generate the aggregate relationship SVG
   */
  generate(): string {
    const aggregates = this.getAggregates();
    if (aggregates.length === 0) {
      return this.generateEmptyState();
    }

    // Group aggregates by context
    const aggregatesByContext = this.groupByContext(aggregates);

    // Calculate layout for all contexts
    const layouts = this.calculateLayouts(aggregatesByContext);

    // Calculate canvas size
    const { width, height } = this.calculateCanvasSize(layouts, aggregatesByContext);

    // Create SVG
    const svg = new SvgBuilder(width, height);
    svg.addShadowFilter();

    // Render title
    this.renderTitle(svg, width);

    // Render each context
    let currentY = this.CANVAS_PADDING + 40;
    for (const [contextName, contextAggregates] of aggregatesByContext) {
      const contextLayouts = layouts.filter(l =>
        contextAggregates.some(a => a.name === l.aggregate.name)
      );
      currentY = this.renderContext(svg, contextName, contextLayouts, currentY, width);
    }

    // Render event flow arrows
    if (this.config.showEventFlow) {
      this.renderEventFlows(svg, layouts);
    }

    // Render legend
    this.renderLegend(svg, width, height);

    return svg.build();
  }

  /** Get aggregates from manifest */
  private getAggregates(): Aggregate[] {
    if (!this.manifest.domain?.aggregates) {
      return [];
    }

    let aggregates = this.manifest.domain.aggregates;

    // Apply context filter if specified
    if (this.config.contextFilter) {
      aggregates = aggregates.filter(
        a => a.context === this.config.contextFilter
      );
    }

    return aggregates;
  }

  /** Group aggregates by their bounded context */
  private groupByContext(aggregates: Aggregate[]): Map<string, Aggregate[]> {
    const groups = new Map<string, Aggregate[]>();

    for (const aggregate of aggregates) {
      const context = aggregate.context || 'Unknown';
      if (!groups.has(context)) {
        groups.set(context, []);
      }
      groups.get(context)!.push(aggregate);
    }

    // Sort by context name
    return new Map([...groups.entries()].sort((a, b) => a[0].localeCompare(b[0])));
  }

  /** Calculate layout positions for all aggregates */
  private calculateLayouts(
    aggregatesByContext: Map<string, Aggregate[]>
  ): AggregateLayout[] {
    const layouts: AggregateLayout[] = [];
    let currentY = this.CANVAS_PADDING + 60; // Account for title

    for (const [, contextAggregates] of aggregatesByContext) {
      // Context header space
      currentY += 40;

      // Calculate positions for aggregates in this context
      let row = 0;
      let col = 0;

      for (const aggregate of contextAggregates) {
        const height = this.calculateAggregateHeight(aggregate);
        const x = this.CANVAS_PADDING + this.CONTEXT_PADDING +
          col * (this.AGGREGATE_WIDTH + this.AGGREGATE_SPACING_X);
        const y = currentY + row * (this.AGGREGATE_MIN_HEIGHT + this.AGGREGATE_SPACING_Y);

        layouts.push({
          aggregate,
          x,
          y,
          width: this.AGGREGATE_WIDTH,
          height,
        });

        col++;
        if (col >= this.config.aggregatesPerRow!) {
          col = 0;
          row++;
        }
      }

      // Move Y for next context
      const rowCount = Math.ceil(contextAggregates.length / this.config.aggregatesPerRow!);
      currentY += rowCount * (this.AGGREGATE_MIN_HEIGHT + this.AGGREGATE_SPACING_Y) + this.CONTEXT_PADDING;
    }

    return layouts;
  }

  /** Calculate the height of an aggregate box */
  private calculateAggregateHeight(aggregate: Aggregate): number {
    if (!this.config.showDetails) {
      return this.AGGREGATE_MIN_HEIGHT;
    }

    let height = this.HEADER_HEIGHT;

    // Add space for entities
    const entities = aggregate.entities || [];
    if (entities.length > 0) {
      height += 25; // Section header
      height += entities.length * this.ENTITY_HEIGHT;
    }

    // Add space for value objects
    const valueObjects = aggregate.value_objects || [];
    if (valueObjects.length > 0) {
      height += 25; // Section header
      height += valueObjects.length * this.ENTITY_HEIGHT;
    }

    // Add space for event handlers
    if (aggregate.event_handlers.length > 0) {
      height += 25; // Section header
      height += Math.min(aggregate.event_handlers.length, 3) * this.ENTITY_HEIGHT;
    }

    return Math.max(height + this.AGGREGATE_PADDING, this.AGGREGATE_MIN_HEIGHT);
  }

  /** Calculate total canvas size */
  private calculateCanvasSize(
    layouts: AggregateLayout[],
    aggregatesByContext: Map<string, Aggregate[]>
  ): { width: number; height: number } {
    if (layouts.length === 0) {
      return { width: 600, height: 300 };
    }

    // Calculate width based on max columns
    const maxCols = Math.min(
      Math.max(...[...aggregatesByContext.values()].map(a => a.length)),
      this.config.aggregatesPerRow!
    );
    const width = this.CANVAS_PADDING * 2 +
      maxCols * this.AGGREGATE_WIDTH +
      (maxCols - 1) * this.AGGREGATE_SPACING_X +
      this.CONTEXT_PADDING * 2;

    // Calculate height based on layouts
    const maxY = Math.max(...layouts.map(l => l.y + l.height));
    const height = maxY + this.CANVAS_PADDING + 80; // Extra space for legend

    return { width: Math.max(width, 600), height: Math.max(height, 400) };
  }

  /** Render the diagram title */
  private renderTitle(svg: SvgBuilder, width: number): void {
    svg.text(
      { x: width / 2, y: 30 },
      'Aggregate Relationships',
      {
        fontSize: 20,
        fontWeight: 'bold',
        fill: AggregateColors.text.primary,
        textAnchor: 'middle',
      }
    );
  }

  /** Render a bounded context section */
  private renderContext(
    svg: SvgBuilder,
    contextName: string,
    layouts: AggregateLayout[],
    startY: number,
    _width: number
  ): number {
    if (layouts.length === 0) return startY;

    // Context header
    svg.text(
      { x: this.CANVAS_PADDING, y: startY },
      `📦 ${contextName}`,
      {
        fontSize: 16,
        fontWeight: 'bold',
        fill: AggregateColors.text.primary,
      }
    );

    // Render each aggregate
    for (const layout of layouts) {
      this.renderAggregate(svg, layout);
    }

    // Calculate next Y position
    const maxY = Math.max(...layouts.map(l => l.y + l.height));
    return maxY + this.AGGREGATE_SPACING_Y;
  }

  /** Render a single aggregate box */
  private renderAggregate(svg: SvgBuilder, layout: AggregateLayout): void {
    const { aggregate, x, y, width, height } = layout;

    // Background rectangle
    svg.rect(
      { x, y },
      { width, height },
      {
        fill: AggregateColors.root,
        stroke: AggregateColors.border,
        strokeWidth: 2,
        rx: 8,
        shadow: true,
      }
    );

    // Aggregate name (root indicator)
    const displayName = aggregate.name.replace(/Aggregate$/, '');
    svg.text(
      { x: x + this.AGGREGATE_PADDING, y: y + 25 },
      `🔷 ${displayName}`,
      {
        fontSize: 14,
        fontWeight: 'bold',
        fill: AggregateColors.text.primary,
      }
    );

    // Folder name if present
    if (aggregate.folder_name) {
      svg.text(
        { x: x + this.AGGREGATE_PADDING, y: y + 42 },
        `(${aggregate.folder_name})`,
        {
          fontSize: 10,
          fill: AggregateColors.text.tertiary,
        }
      );
    }

    if (!this.config.showDetails) return;

    let currentY = y + this.HEADER_HEIGHT;

    // Entities section
    const entities = aggregate.entities || [];
    if (entities.length > 0) {
      svg.text(
        { x: x + this.AGGREGATE_PADDING, y: currentY },
        'Entities:',
        {
          fontSize: 11,
          fontWeight: 'bold',
          fill: AggregateColors.text.secondary,
        }
      );
      currentY += 18;

      for (const entity of entities) {
        this.renderEntity(svg, entity, x + this.AGGREGATE_PADDING + 5, currentY);
        currentY += this.ENTITY_HEIGHT;
      }
      currentY += 5;
    }

    // Value Objects section
    const valueObjects = aggregate.value_objects || [];
    if (valueObjects.length > 0) {
      svg.text(
        { x: x + this.AGGREGATE_PADDING, y: currentY },
        'Value Objects:',
        {
          fontSize: 11,
          fontWeight: 'bold',
          fill: AggregateColors.text.secondary,
        }
      );
      currentY += 18;

      for (const vo of valueObjects) {
        this.renderValueObject(svg, vo, x + this.AGGREGATE_PADDING + 5, currentY);
        currentY += this.ENTITY_HEIGHT;
      }
      currentY += 5;
    }

    // Event handlers (subscriptions)
    if (aggregate.event_handlers.length > 0) {
      svg.text(
        { x: x + this.AGGREGATE_PADDING, y: currentY },
        'Subscribes to:',
        {
          fontSize: 11,
          fontWeight: 'bold',
          fill: AggregateColors.text.secondary,
        }
      );
      currentY += 18;

      const handlers = aggregate.event_handlers.slice(0, 3);
      for (const handler of handlers) {
        svg.text(
          { x: x + this.AGGREGATE_PADDING + 5, y: currentY },
          `← ${handler.event_type}`,
          {
            fontSize: 10,
            fill: AggregateColors.subscribes,
          }
        );
        currentY += this.ENTITY_HEIGHT;
      }

      if (aggregate.event_handlers.length > 3) {
        svg.text(
          { x: x + this.AGGREGATE_PADDING + 5, y: currentY },
          `... and ${aggregate.event_handlers.length - 3} more`,
          {
            fontSize: 10,
            fill: AggregateColors.text.tertiary,
          }
        );
      }
    }
  }

  /** Render an entity indicator */
  private renderEntity(svg: SvgBuilder, entity: AggregateEntity, x: number, y: number): void {
    // Small rectangle for entity
    svg.rect(
      { x, y: y - 12 },
      { width: 12, height: 12 },
      {
        fill: AggregateColors.entity,
        stroke: AggregateColors.border,
        strokeWidth: 1,
        rx: 2,
      }
    );

    const identityText = entity.identity_field ? ` (${entity.identity_field})` : '';
    svg.text(
      { x: x + 16, y: y - 2 },
      `${entity.name}${identityText}`,
      {
        fontSize: 10,
        fill: AggregateColors.text.secondary,
      }
    );
  }

  /** Render a value object indicator */
  private renderValueObject(
    svg: SvgBuilder,
    vo: AggregateValueObject,
    x: number,
    y: number
  ): void {
    // Small circle for value object
    const immutableIndicator = vo.is_immutable ? '❄️' : '';
    svg.text(
      { x, y: y - 2 },
      `◇ ${vo.name} ${immutableIndicator}`,
      {
        fontSize: 10,
        fill: AggregateColors.text.secondary,
      }
    );
  }

  /** Render event flow arrows between aggregates */
  private renderEventFlows(svg: SvgBuilder, layouts: AggregateLayout[]): void {
    // Build event emission map
    const eventEmitters = new Map<string, AggregateLayout>();
    const eventSubscribers = new Map<string, AggregateLayout[]>();

    for (const layout of layouts) {
      // Track what events this aggregate emits
      for (const handler of layout.aggregate.command_handlers) {
        for (const event of handler.emits_events) {
          eventEmitters.set(event, layout);
        }
      }

      // Track what events this aggregate subscribes to
      for (const handler of layout.aggregate.event_handlers) {
        if (!eventSubscribers.has(handler.event_type)) {
          eventSubscribers.set(handler.event_type, []);
        }
        eventSubscribers.get(handler.event_type)!.push(layout);
      }
    }

    // Draw arrows from emitters to subscribers
    for (const [eventType, emitter] of eventEmitters) {
      const subscribers = eventSubscribers.get(eventType) || [];

      for (const subscriber of subscribers) {
        // Don't draw self-loops
        if (emitter.aggregate.name === subscriber.aggregate.name) continue;

        this.renderArrow(svg, emitter, subscriber, eventType);
      }
    }
  }

  /** Render an arrow between two aggregate boxes */
  private renderArrow(
    svg: SvgBuilder,
    from: AggregateLayout,
    to: AggregateLayout,
    label: string
  ): void {
    // Calculate connection points
    const fromCenter: Point = {
      x: from.x + from.width / 2,
      y: from.y + from.height / 2,
    };
    const toCenter: Point = {
      x: to.x + to.width / 2,
      y: to.y + to.height / 2,
    };

    // Determine which sides to connect
    let startPoint: Point;
    let endPoint: Point;

    const dx = toCenter.x - fromCenter.x;
    const dy = toCenter.y - fromCenter.y;

    if (Math.abs(dx) > Math.abs(dy)) {
      // Horizontal connection
      if (dx > 0) {
        startPoint = { x: from.x + from.width, y: fromCenter.y };
        endPoint = { x: to.x, y: toCenter.y };
      } else {
        startPoint = { x: from.x, y: fromCenter.y };
        endPoint = { x: to.x + to.width, y: toCenter.y };
      }
    } else {
      // Vertical connection
      if (dy > 0) {
        startPoint = { x: fromCenter.x, y: from.y + from.height };
        endPoint = { x: toCenter.x, y: to.y };
      } else {
        startPoint = { x: fromCenter.x, y: from.y };
        endPoint = { x: toCenter.x, y: to.y + to.height };
      }
    }

    // Draw the arrow line
    svg.line(startPoint, endPoint, AggregateColors.emits, 2);

    // Note: Arrowhead skipped for simplicity - direction implied by label

    // Add label at midpoint
    const midPoint: Point = {
      x: (startPoint.x + endPoint.x) / 2,
      y: (startPoint.y + endPoint.y) / 2 - 5,
    };

    // Truncate long event names
    const displayLabel = label.length > 20 ? label.substring(0, 17) + '...' : label;

    svg.text(midPoint, displayLabel, {
      fontSize: 9,
      fill: AggregateColors.emits,
      textAnchor: 'middle',
    });
  }

  /** Render the legend */
  private renderLegend(svg: SvgBuilder, width: number, height: number): void {
    const legendY = height - 50;
    const legendX = this.CANVAS_PADDING;

    svg.text(
      { x: legendX, y: legendY },
      'Legend:',
      {
        fontSize: 11,
        fontWeight: 'bold',
        fill: AggregateColors.text.secondary,
      }
    );

    // Aggregate root
    svg.rect(
      { x: legendX + 60, y: legendY - 12 },
      { width: 14, height: 14 },
      {
        fill: AggregateColors.root,
        stroke: AggregateColors.border,
        rx: 3,
      }
    );
    svg.text(
      { x: legendX + 80, y: legendY },
      '🔷 Aggregate Root',
      {
        fontSize: 10,
        fill: AggregateColors.text.secondary,
      }
    );

    // Entity
    svg.rect(
      { x: legendX + 200, y: legendY - 12 },
      { width: 14, height: 14 },
      {
        fill: AggregateColors.entity,
        stroke: AggregateColors.border,
        rx: 2,
      }
    );
    svg.text(
      { x: legendX + 220, y: legendY },
      'Entity',
      {
        fontSize: 10,
        fill: AggregateColors.text.secondary,
      }
    );

    // Value Object
    svg.text(
      { x: legendX + 290, y: legendY },
      '◇ Value Object',
      {
        fontSize: 10,
        fill: AggregateColors.text.secondary,
      }
    );

    // Event arrow
    if (this.config.showEventFlow) {
      svg.line(
        { x: legendX + 400, y: legendY - 5 },
        { x: legendX + 430, y: legendY - 5 },
        AggregateColors.emits,
        2
      );
      svg.text(
        { x: legendX + 435, y: legendY },
        '→ Event Flow',
        {
          fontSize: 10,
          fill: AggregateColors.text.secondary,
        }
      );
    }
  }

  /** Generate empty state SVG */
  private generateEmptyState(): string {
    const svg = new SvgBuilder(600, 200);

    svg.rect(
      { x: 0, y: 0 },
      { width: 600, height: 200 },
      {
        fill: '#f9f9f9',
      }
    );

    svg.text(
      { x: 300, y: 80 },
      'No Aggregates Found',
      {
        fontSize: 18,
        fontWeight: 'bold',
        fill: '#666666',
        textAnchor: 'middle',
      }
    );

    svg.text(
      { x: 300, y: 110 },
      'Use --include-domain flag when generating manifest',
      {
        fontSize: 12,
        fill: '#999999',
        textAnchor: 'middle',
      }
    );

    svg.text(
      { x: 300, y: 130 },
      'Ensure aggregates follow *Aggregate.* naming convention',
      {
        fontSize: 12,
        fill: '#999999',
        textAnchor: 'middle',
      }
    );

    return svg.build();
  }
}
