/**
 * Architecture SVG Generator
 * 
 * Generates SiYuan-style architecture diagrams in SVG format from VSA manifests.
 * Creates a clean, professional visualization showing:
 * - Applications layer (top)
 * - Domain contexts (middle, in grid layout)
 * - Infrastructure layer (bottom)
 * - Ecosystem sidebar (right)
 */

import { BaseGenerator } from './base-generator';
import { Manifest, BoundedContext, Feature } from '../types/manifest';
import { SvgBuilder, Point, ArchitectureColors } from '../utils/svg-builder';

interface GridLayout {
  cols: number;
  rows: number;
  cellWidth: number;
  cellHeight: number;
}

interface LayerConfig {
  applications?: string[];
  infrastructure?: Array<{ name: string; description?: string }>;
  packages?: string[];
  libraries?: Array<{ name: string; repo?: string }>;
}

export class ArchitectureSvgGenerator extends BaseGenerator {
  // Canvas dimensions
  private readonly CANVAS_WIDTH = 1400;
  private readonly BASE_CANVAS_HEIGHT = 1000;
  
  // Layout constants
  private readonly PADDING = 40;
  private readonly SECTION_MARGIN = 20;
  private readonly MAX_COLS = 3;
  private readonly CONTEXT_CELL_HEIGHT = 180;
  private readonly SIDEBAR_WIDTH = 280;
  private readonly CQRS_LAYER_HEIGHT = 200;
  private readonly CQRS_BOX_HEIGHT = 120;
  
  // Typography
  private readonly FONT_HEADER = 'Arial, Helvetica, sans-serif';
  private readonly FONT_SIZE_TITLE = 24;
  private readonly FONT_SIZE_SECTION = 16;
  private readonly FONT_SIZE_CONTEXT = 14;
  private readonly FONT_SIZE_FEATURE = 12;
  
  // Color scheme
  private readonly colors = ArchitectureColors;
  
  // CQRS colors
  private readonly CQRS_COLORS = {
    command: { fill: '#e3f2fd', stroke: '#1976d2' },
    event: { fill: '#fff3e0', stroke: '#f57c00' },
    projection: { fill: '#e8f5e9', stroke: '#388e3c' },
  };
  
  private layerConfig: LayerConfig = {};
  
  constructor(manifest: Manifest, config?: LayerConfig) {
    super(manifest);
    if (config) {
      this.layerConfig = config;
    }
    
    // Infer layer config from manifest if not provided
    this.inferLayerConfig();
  }
  
  protected getTitle(): string {
    return 'Architecture Diagram';
  }
  
  /**
   * Calculate required canvas height based on layers to be rendered
   */
  private calculateCanvasHeight(): number {
    let height = this.BASE_CANVAS_HEIGHT;
    
    // Add extra height for CQRS layer if it will be rendered
    if (this.shouldRenderCqrsLayer()) {
      height += this.CQRS_LAYER_HEIGHT;
    }
    
    return height;
  }
  
  /**
   * Generate the SVG diagram
   */
  generate(): string {
    const canvasHeight = this.calculateCanvasHeight();
    const svg = new SvgBuilder(this.CANVAS_WIDTH, canvasHeight);
    
    // Add shadow filter
    svg.addShadowFilter();
    
    // Add global styles
    svg.addStyle(`
      .context-box { cursor: pointer; }
      .context-box:hover { opacity: 0.9; }
    `);
    
    // Render layers from top to bottom
    let currentY = this.PADDING;
    
    // 1. Header
    currentY = this.renderHeader(svg, currentY);
    currentY += this.SECTION_MARGIN;
    
    // 2. Applications layer (if configured)
    if (this.layerConfig.applications && this.layerConfig.applications.length > 0) {
      currentY = this.renderApplicationsLayer(svg, currentY);
      currentY += this.SECTION_MARGIN * 2;
    }
    
    // 3. Domain contexts (main section)
    const contextsEndY = this.renderContextsLayer(svg, currentY);
    let infraStartY = contextsEndY + this.SECTION_MARGIN * 2;
    
    // 4. CQRS Pattern layer (if domain data available)
    if (this.shouldRenderCqrsLayer()) {
      infraStartY = this.renderCqrsLayer(svg, infraStartY);
      infraStartY += this.SECTION_MARGIN * 2;
    }
    
    // 5. Infrastructure layer (if configured)
    if (this.layerConfig.infrastructure && this.layerConfig.infrastructure.length > 0) {
      this.renderInfrastructureLayer(svg, infraStartY);
    }
    
    // 6. Ecosystem sidebar (right side)
    this.renderEcosystemSidebar(svg);
    
    return svg.build();
  }
  
  /**
   * Render the header with title
   */
  private renderHeader(svg: SvgBuilder, startY: number): number {
    const centerX = (this.CANVAS_WIDTH - this.SIDEBAR_WIDTH - this.PADDING) / 2 + this.PADDING;
    
    svg.text(
      { x: centerX, y: startY + this.FONT_SIZE_TITLE },
      '🏗️ Architecture and Ecosystem',
      {
        fontSize: this.FONT_SIZE_TITLE,
        fontWeight: 'bold',
        fontFamily: this.FONT_HEADER,
        fill: this.colors.text.primary,
        textAnchor: 'middle'
      }
    );
    
    // Horizontal line under header
    const lineY = startY + this.FONT_SIZE_TITLE + 10;
    svg.line(
      { x: this.PADDING, y: lineY },
      { x: this.CANVAS_WIDTH - this.SIDEBAR_WIDTH - this.PADDING, y: lineY },
      this.colors.border.light,
      2
    );
    
    return lineY + 10;
  }
  
  /**
   * Render applications layer (CLI, Dashboard, UI, etc.)
   */
  private renderApplicationsLayer(svg: SvgBuilder, startY: number): number {
    const applications = this.layerConfig.applications || [];
    if (applications.length === 0) return startY;
    
    const availableWidth = this.CANVAS_WIDTH - this.SIDEBAR_WIDTH - (this.PADDING * 2) - this.SECTION_MARGIN;
    const boxWidth = Math.min(200, (availableWidth - ((applications.length - 1) * 10)) / applications.length);
    const boxHeight = 60;
    
    // Section title
    svg.text(
      { x: this.PADDING, y: startY },
      'Applications',
      {
        fontSize: this.FONT_SIZE_SECTION,
        fontWeight: 'bold',
        fontFamily: this.FONT_HEADER,
        fill: this.colors.text.secondary
      }
    );
    
    const boxStartY = startY + 10;
    let currentX = this.PADDING;
    
    applications.forEach(app => {
      // Application box
      svg.rect(
        { x: currentX, y: boxStartY },
        { width: boxWidth, height: boxHeight },
        {
          fill: this.colors.application.primary,
          stroke: this.colors.border.medium,
          strokeWidth: 1,
          rx: 8,
          shadow: true
        }
      );
      
      // Application name (centered)
      svg.text(
        { x: currentX + boxWidth / 2, y: boxStartY + boxHeight / 2 + 5 },
        app,
        {
          fontSize: this.FONT_SIZE_CONTEXT,
          fontWeight: '500',
          fontFamily: this.FONT_HEADER,
          fill: this.colors.text.primary,
          textAnchor: 'middle'
        }
      );
      
      currentX += boxWidth + 10;
    });
    
    return boxStartY + boxHeight + this.SECTION_MARGIN;
  }
  
  /**
   * Render domain contexts in grid layout
   */
  private renderContextsLayer(svg: SvgBuilder, startY: number): number {
    const contexts = this.manifest.bounded_contexts || [];
    if (contexts.length === 0) return startY;
    
    const availableWidth = this.CANVAS_WIDTH - this.SIDEBAR_WIDTH - (this.PADDING * 2) - this.SECTION_MARGIN;
    const grid = this.calculateGrid(contexts.length, availableWidth);
    
    // Section title
    svg.text(
      { x: this.PADDING, y: startY },
      'Domain Contexts (Bounded Contexts)',
      {
        fontSize: this.FONT_SIZE_SECTION,
        fontWeight: 'bold',
        fontFamily: this.FONT_HEADER,
        fill: this.colors.text.secondary
      }
    );
    
    const gridStartY = startY + 10;
    
    contexts.forEach((context, index) => {
      const pos = this.getGridPosition(index, grid, this.PADDING, gridStartY);
      this.renderContext(svg, context, pos, grid.cellWidth, this.CONTEXT_CELL_HEIGHT, index);
    });
    
    return gridStartY + (grid.rows * (this.CONTEXT_CELL_HEIGHT + 15));
  }
  
  /**
   * Render a single context box
   */
  private renderContext(
    svg: SvgBuilder,
    context: BoundedContext,
    pos: Point,
    width: number,
    height: number,
    index: number
  ): void {
    // Cycle through colors
    const colorIndex = index % 3;
    const fillColors = [
      this.colors.context.primary,
      this.colors.context.secondary,
      this.colors.context.tertiary
    ];
    
    // Context box
    svg.rect(
      pos,
      { width, height },
      {
        fill: fillColors[colorIndex],
        stroke: this.colors.border.medium,
        strokeWidth: 1.5,
        rx: 8,
        shadow: true
      }
    );
    
    // Context name
    svg.text(
      { x: pos.x + 15, y: pos.y + 25 },
      context.name,
      {
        fontSize: this.FONT_SIZE_CONTEXT,
        fontWeight: 'bold',
        fontFamily: this.FONT_HEADER,
        fill: this.colors.text.primary
      }
    );
    
    // Feature count
    const features = this.filterFeatures(context.features || []);
    svg.text(
      { x: pos.x + 15, y: pos.y + 45 },
      `(${features.length} feature${features.length !== 1 ? 's' : ''})`,
      {
        fontSize: this.FONT_SIZE_FEATURE,
        fontFamily: this.FONT_HEADER,
        fill: this.colors.text.secondary
      }
    );
    
    // List top features
    const maxFeatures = 5;
    const displayFeatures = features.slice(0, maxFeatures);
    let featureY = pos.y + 65;
    
    displayFeatures.forEach(feature => {
      svg.text(
        { x: pos.x + 15, y: featureY },
        `• ${feature.name}`,
        {
          fontSize: this.FONT_SIZE_FEATURE,
          fontFamily: this.FONT_HEADER,
          fill: this.colors.text.primary
        }
      );
      featureY += 18;
    });
    
    // Show "..." if there are more features
    if (features.length > maxFeatures) {
      svg.text(
        { x: pos.x + 15, y: featureY },
        `• ... and ${features.length - maxFeatures} more`,
        {
          fontSize: this.FONT_SIZE_FEATURE,
          fontFamily: this.FONT_HEADER,
          fill: this.colors.text.tertiary,
          fontWeight: 'italic'
        }
      );
    }
  }
  
  /**
   * Render infrastructure layer (databases, caches, etc.)
   */
  private renderInfrastructureLayer(svg: SvgBuilder, startY: number): number {
    const infrastructure = this.layerConfig.infrastructure || [];
    if (infrastructure.length === 0) return startY;
    
    const availableWidth = this.CANVAS_WIDTH - this.SIDEBAR_WIDTH - (this.PADDING * 2) - this.SECTION_MARGIN;
    const boxWidth = Math.min(180, (availableWidth - ((infrastructure.length - 1) * 10)) / infrastructure.length);
    const boxHeight = 70;
    
    // Section title
    svg.text(
      { x: this.PADDING, y: startY },
      'Infrastructure',
      {
        fontSize: this.FONT_SIZE_SECTION,
        fontWeight: 'bold',
        fontFamily: this.FONT_HEADER,
        fill: this.colors.text.secondary
      }
    );
    
    const boxStartY = startY + 10;
    let currentX = this.PADDING;
    
    infrastructure.forEach(infra => {
      // Infrastructure box
      svg.rect(
        { x: currentX, y: boxStartY },
        { width: boxWidth, height: boxHeight },
        {
          fill: this.colors.infrastructure.primary,
          stroke: this.colors.border.medium,
          strokeWidth: 1,
          rx: 6,
          shadow: true
        }
      );
      
      // Name
      svg.text(
        { x: currentX + boxWidth / 2, y: boxStartY + 25 },
        infra.name,
        {
          fontSize: this.FONT_SIZE_CONTEXT,
          fontWeight: '500',
          fontFamily: this.FONT_HEADER,
          fill: this.colors.text.primary,
          textAnchor: 'middle'
        }
      );
      
      // Description (if provided)
      if (infra.description) {
        svg.text(
          { x: currentX + boxWidth / 2, y: boxStartY + 45 },
          infra.description,
          {
            fontSize: this.FONT_SIZE_FEATURE,
            fontFamily: this.FONT_HEADER,
            fill: this.colors.text.secondary,
            textAnchor: 'middle'
          }
        );
      }
      
      currentX += boxWidth + 10;
    });
    
    return boxStartY + boxHeight;
  }
  
  /**
   * Render ecosystem sidebar (packages and libraries)
   */
  private renderEcosystemSidebar(svg: SvgBuilder): void {
    const sidebarX = this.CANVAS_WIDTH - this.SIDEBAR_WIDTH + this.SECTION_MARGIN;
    let currentY = this.PADDING + 60; // Start below header
    
    const packages = this.layerConfig.packages || [];
    const libraries = this.layerConfig.libraries || [];
    
    if (packages.length === 0 && libraries.length === 0) return;
    
    // Packages section
    if (packages.length > 0) {
      svg.text(
        { x: sidebarX, y: currentY },
        'Packages',
        {
          fontSize: this.FONT_SIZE_SECTION,
          fontWeight: 'bold',
          fontFamily: this.FONT_HEADER,
          fill: this.colors.text.secondary
        }
      );
      currentY += 20;
      
      const packageBoxHeight = packages.length * 22 + 20;
      svg.rect(
        { x: sidebarX, y: currentY },
        { width: this.SIDEBAR_WIDTH - this.SECTION_MARGIN * 2, height: packageBoxHeight },
        {
          fill: this.colors.ecosystem.primary,
          stroke: this.colors.border.light,
          strokeWidth: 1,
          rx: 6
        }
      );
      
      currentY += 20;
      packages.forEach(pkg => {
        svg.text(
          { x: sidebarX + 15, y: currentY },
          `• ${pkg}`,
          {
            fontSize: this.FONT_SIZE_FEATURE,
            fontFamily: this.FONT_HEADER,
            fill: this.colors.text.primary
          }
        );
        currentY += 22;
      });
      
      currentY += 20;
    }
    
    // Libraries section
    if (libraries.length > 0) {
      svg.text(
        { x: sidebarX, y: currentY },
        'Libraries',
        {
          fontSize: this.FONT_SIZE_SECTION,
          fontWeight: 'bold',
          fontFamily: this.FONT_HEADER,
          fill: this.colors.text.secondary
        }
      );
      currentY += 20;
      
      libraries.forEach(lib => {
        const boxHeight = lib.repo ? 65 : 45;
        
        svg.rect(
          { x: sidebarX, y: currentY },
          { width: this.SIDEBAR_WIDTH - this.SECTION_MARGIN * 2, height: boxHeight },
          {
            fill: this.colors.ecosystem.secondary,
            stroke: this.colors.border.light,
            strokeWidth: 1,
            rx: 6
          }
        );
        
        svg.text(
          { x: sidebarX + 15, y: currentY + 25 },
          lib.name,
          {
            fontSize: this.FONT_SIZE_CONTEXT,
            fontWeight: '500',
            fontFamily: this.FONT_HEADER,
            fill: this.colors.text.primary
          }
        );
        
        if (lib.repo) {
          svg.text(
            { x: sidebarX + 15, y: currentY + 45 },
            lib.repo,
            {
              fontSize: this.FONT_SIZE_FEATURE - 1,
              fontFamily: this.FONT_HEADER,
              fill: this.colors.text.tertiary
            }
          );
        }
        
        currentY += boxHeight + 10;
      });
    }
  }
  
  /**
   * Calculate grid layout for contexts
   */
  private calculateGrid(itemCount: number, availableWidth: number): GridLayout {
    const cols = Math.min(itemCount, this.MAX_COLS);
    const rows = Math.ceil(itemCount / cols);
    const cellWidth = (availableWidth - ((cols - 1) * 15)) / cols;
    
    return {
      cols,
      rows,
      cellWidth,
      cellHeight: this.CONTEXT_CELL_HEIGHT
    };
  }
  
  /**
   * Get position for item in grid
   */
  private getGridPosition(index: number, grid: GridLayout, startX: number, startY: number): Point {
    const col = index % grid.cols;
    const row = Math.floor(index / grid.cols);
    
    return {
      x: startX + col * (grid.cellWidth + 15),
      y: startY + row * (this.CONTEXT_CELL_HEIGHT + 15)
    };
  }
  
  /**
   * Filter out infrastructure and organizational folders from features
   */
  private filterFeatures(features: Feature[]): Feature[] {
    const excluded = ['domain', 'slices', 'ports', 'application', 'events', 'commands', 'queries', 'read_models'];
    return features.filter(f => !excluded.includes(f.name) && !f.name.startsWith('_'));
  }
  
  /**
   * Infer layer configuration from manifest if not explicitly provided
   */
  private inferLayerConfig(): void {
    // Infer packages from context paths
    if (!this.layerConfig.packages && this.manifest.bounded_contexts) {
      const packageSet = new Set<string>();
      this.manifest.bounded_contexts.forEach(context => {
        const pathParts = context.path.split('/');
        const packagesIndex = pathParts.indexOf('packages');
        if (packagesIndex !== -1 && packagesIndex + 1 < pathParts.length) {
          packageSet.add(pathParts[packagesIndex + 1]);
        }
      });
      if (packageSet.size > 0) {
        this.layerConfig.packages = Array.from(packageSet).sort();
      }
    }
    
    // Set default infrastructure if not provided
    if (!this.layerConfig.infrastructure) {
      this.layerConfig.infrastructure = [
        { name: 'PostgreSQL', description: 'Event Store' },
        { name: 'Redis', description: 'Cache' },
        { name: 'MinIO', description: 'Artifacts' }
      ];
    }
  }
  
  /**
   * Check if CQRS layer should be rendered (if domain data is available)
   */
  private shouldRenderCqrsLayer(): boolean {
    return !!(
      this.manifest.domain &&
      (this.manifest.domain.commands.length > 0 ||
       this.manifest.domain.events.length > 0 ||
       (this.manifest.domain.projections && this.manifest.domain.projections.length > 0))
    );
  }
  
  /**
   * Render CQRS Pattern layer showing Commands → Events → Projections
   */
  private renderCqrsLayer(svg: SvgBuilder, startY: number): number {
    const availableWidth = this.CANVAS_WIDTH - this.SIDEBAR_WIDTH - (this.PADDING * 2);
    const boxWidth = (availableWidth - 40) / 3; // 3 boxes with spacing
    const boxHeight = this.CQRS_BOX_HEIGHT;
    const boxY = startY + 60;
    const spacing = 20;
    
    // Section title
    svg.text(
      { x: this.PADDING, y: startY + 30 },
      'CQRS Pattern',
      {
        fontSize: this.FONT_SIZE_SECTION,
        fontWeight: 'bold',
        fontFamily: this.FONT_HEADER,
        fill: this.colors.text.primary
      }
    );
    
    const domain = this.manifest.domain!;
    const commandCount = domain.commands.length;
    const eventCount = domain.events.length;
    const projectionCount = domain.projections?.length || 0;
    
    // Calculate box positions
    const commandX = this.PADDING;
    const eventX = commandX + boxWidth + spacing;
    const projectionX = eventX + boxWidth + spacing;
    
    // Commands box
    this.renderCqrsBox(
      svg,
      { x: commandX, y: boxY },
      { width: boxWidth, height: boxHeight },
      'Commands',
      commandCount,
      this.CQRS_COLORS.command
    );
    
    // Events box
    this.renderCqrsBox(
      svg,
      { x: eventX, y: boxY },
      { width: boxWidth, height: boxHeight },
      'Events',
      eventCount,
      this.CQRS_COLORS.event
    );
    
    // Projections box
    this.renderCqrsBox(
      svg,
      { x: projectionX, y: boxY },
      { width: boxWidth, height: boxHeight },
      'Projections',
      projectionCount,
      this.CQRS_COLORS.projection
    );
    
    // Arrows
    const arrowY = boxY + boxHeight / 2;
    const arrowColor = '#666666';
    
    // Command → Event arrow
    svg.line(
      { x: commandX + boxWidth, y: arrowY },
      { x: eventX, y: arrowY },
      arrowColor,
      2
    );
    this.renderArrowHead(svg, { x: eventX - 5, y: arrowY }, arrowColor);
    
    // Event → Projection arrow
    svg.line(
      { x: eventX + boxWidth, y: arrowY },
      { x: projectionX, y: arrowY },
      arrowColor,
      2
    );
    this.renderArrowHead(svg, { x: projectionX - 5, y: arrowY }, arrowColor);
    
    return startY + this.CQRS_LAYER_HEIGHT;
  }
  
  /**
   * Render a single CQRS box (Command, Event, or Projection)
   */
  private renderCqrsBox(
    svg: SvgBuilder,
    pos: Point,
    dims: { width: number; height: number },
    label: string,
    count: number,
    colors: { fill: string; stroke: string }
  ): void {
    // Box
    svg.rect(pos, dims, {
      fill: colors.fill,
      stroke: colors.stroke,
      strokeWidth: 2,
      rx: 8
    });
    
    // Label
    svg.text(
      { x: pos.x + 20, y: pos.y + 35 },
      label,
      {
        fontSize: 18,
        fontWeight: 'bold',
        fontFamily: this.FONT_HEADER,
        fill: this.colors.text.primary
      }
    );
    
    // Count
    svg.text(
      { x: pos.x + 20, y: pos.y + 65 },
      `${count} total`,
      {
        fontSize: 14,
        fontFamily: this.FONT_HEADER,
        fill: '#666666'
      }
    );
  }
  
  /**
   * Render an arrow head
   */
  private renderArrowHead(svg: SvgBuilder, pos: Point, color: string): void {
    const size = 8;
    svg._addElement(
      `<polygon points="${pos.x},${pos.y} ${pos.x - size},${pos.y - size / 2} ${pos.x - size},${pos.y + size / 2}" fill="${color}" />`
    );
  }
}
