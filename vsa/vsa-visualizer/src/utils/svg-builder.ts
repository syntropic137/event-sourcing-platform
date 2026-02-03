/**
 * SVG Builder Utility
 *
 * Helper class for generating SVG elements with clean, readable code.
 * Provides methods for creating common SVG shapes, text, and document structure.
 */

export interface Point {
  x: number;
  y: number;
}

export interface Dimensions {
  width: number;
  height: number;
}

export interface RectStyle {
  fill?: string;
  stroke?: string;
  strokeWidth?: number;
  rx?: number;  // border radius
  opacity?: number;
  shadow?: boolean;
}

export interface TextStyle {
  fontSize?: number;
  fontWeight?: string;
  fontFamily?: string;
  fill?: string;
  textAnchor?: 'start' | 'middle' | 'end';
}

export class SvgBuilder {
  private elements: string[] = [];
  private defs: string[] = [];
  private styles: string[] = [];

  constructor(
    private width: number,
    private height: number
  ) {}

  /**
   * Build the complete SVG document
   */
  build(): string {
    const xmlns = 'http://www.w3.org/2000/svg';
    const viewBox = `0 0 ${this.width} ${this.height}`;

    let svg = `<svg xmlns="${xmlns}" viewBox="${viewBox}" width="${this.width}" height="${this.height}">\n`;

    // Add defs section if we have any
    if (this.defs.length > 0) {
      svg += '  <defs>\n';
      svg += this.defs.map(d => `    ${d}`).join('\n') + '\n';
      svg += '  </defs>\n';
    }

    // Add styles section if we have any
    if (this.styles.length > 0) {
      svg += '  <style>\n';
      svg += this.styles.map(s => `    ${s}`).join('\n') + '\n';
      svg += '  </style>\n';
    }

    // Add all elements
    svg += this.elements.map(e => `  ${e}`).join('\n') + '\n';
    svg += '</svg>';

    return svg;
  }

  /**
   * Add a drop shadow filter definition
   */
  addShadowFilter(id: string = 'shadow'): this {
    const filter = `
<filter id="${id}" x="-50%" y="-50%" width="200%" height="200%">
  <feGaussianBlur in="SourceAlpha" stdDeviation="2"/>
  <feOffset dx="0" dy="2" result="offsetblur"/>
  <feComponentTransfer>
    <feFuncA type="linear" slope="0.2"/>
  </feComponentTransfer>
  <feMerge>
    <feMergeNode/>
    <feMergeNode in="SourceGraphic"/>
  </feMerge>
</filter>`.trim();

    this.defs.push(filter);
    return this;
  }

  /**
   * Add a CSS style
   */
  addStyle(css: string): this {
    this.styles.push(css);
    return this;
  }

  /**
   * Add a rectangle
   */
  rect(pos: Point, dims: Dimensions, style: RectStyle = {}): this {
    const {
      fill = '#ffffff',
      stroke = '#cccccc',
      strokeWidth = 1,
      rx = 0,
      opacity = 1,
      shadow = false
    } = style;

    const filter = shadow ? ' filter="url(#shadow)"' : '';

    const rect =
      `<rect x="${pos.x}" y="${pos.y}" width="${dims.width}" height="${dims.height}" ` +
      `fill="${fill}" stroke="${stroke}" stroke-width="${strokeWidth}" ` +
      `rx="${rx}" opacity="${opacity}"${filter}/>`;

    this.elements.push(rect);
    return this;
  }

  /**
   * Add text
   */
  text(pos: Point, content: string, style: TextStyle = {}): this {
    const {
      fontSize = 14,
      fontWeight = 'normal',
      fontFamily = 'Arial, Helvetica, sans-serif',
      fill = '#000000',
      textAnchor = 'start'
    } = style;

    // Escape special XML characters
    const escapedContent = content
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&apos;');

    const text =
      `<text x="${pos.x}" y="${pos.y}" ` +
      `font-size="${fontSize}" font-weight="${fontWeight}" ` +
      `font-family="${fontFamily}" fill="${fill}" text-anchor="${textAnchor}">${escapedContent}</text>`;

    this.elements.push(text);
    return this;
  }

  /**
   * Add multiple lines of text (each line is a separate text element)
   */
  multilineText(pos: Point, lines: string[], style: TextStyle = {}, lineHeight: number = 20): this {
    lines.forEach((line, index) => {
      this.text(
        { x: pos.x, y: pos.y + (index * lineHeight) },
        line,
        style
      );
    });
    return this;
  }

  /**
   * Add a line
   */
  line(start: Point, end: Point, stroke: string = '#cccccc', strokeWidth: number = 1): this {
    const line = `<line x1="${start.x}" y1="${start.y}" x2="${end.x}" y2="${end.y}" ` +
                 `stroke="${stroke}" stroke-width="${strokeWidth}"/>`;
    this.elements.push(line);
    return this;
  }

  /**
   * Add a group (allows applying transforms, classes, etc. to multiple elements)
   */
  group(id?: string, transform?: string): GroupBuilder {
    return new GroupBuilder(this, id, transform);
  }

  /**
   * Internal: Add a raw SVG element (used by GroupBuilder)
   */
  _addElement(element: string): void {
    this.elements.push(element);
  }
}

/**
 * Helper class for building SVG groups
 */
export class GroupBuilder {
  private elements: string[] = [];

  constructor(
    private parent: SvgBuilder,
    private id?: string,
    private transform?: string
  ) {}

  /**
   * Add a rectangle to the group
   */
  rect(pos: Point, dims: Dimensions, style: RectStyle = {}): this {
    const builder = new SvgBuilder(0, 0);
    builder.rect(pos, dims, style);
    const element = builder['elements'][0].replace(/^ {2}/, '');
    this.elements.push(`    ${element}`);
    return this;
  }

  /**
   * Add text to the group
   */
  text(pos: Point, content: string, style: TextStyle = {}): this {
    const builder = new SvgBuilder(0, 0);
    builder.text(pos, content, style);
    const element = builder['elements'][0].replace(/^ {2}/, '');
    this.elements.push(`    ${element}`);
    return this;
  }

  /**
   * Close the group and add it to the parent builder
   */
  end(): SvgBuilder {
    const idAttr = this.id ? ` id="${this.id}"` : '';
    const transformAttr = this.transform ? ` transform="${this.transform}"` : '';

    const group = `<g${idAttr}${transformAttr}>\n${this.elements.join('\n')}\n  </g>`;
    this.parent._addElement(group);

    return this.parent;
  }
}

/**
 * Color palette for architecture diagrams
 */
export const ArchitectureColors = {
  // Bounded contexts - Pastel colors
  context: {
    primary: '#d6c9ff',      // Pastel purple
    secondary: '#c9d9ff',    // Pastel blue
    tertiary: '#ffd9e8',     // Pastel pink
  },

  // Infrastructure layer
  infrastructure: {
    primary: '#cce5ff',      // Light blue
    secondary: '#d9f0ff',
  },

  // Applications layer
  application: {
    primary: '#e8e8e8',      // Light gray
    secondary: '#f5f5f5',
  },

  // Packages/Libraries ecosystem
  ecosystem: {
    primary: '#d4f1d4',      // Light green
    secondary: '#e5f7e5',
  },

  // Text
  text: {
    primary: '#1a1a1a',      // Almost black
    secondary: '#666666',    // Medium gray
    tertiary: '#999999',     // Light gray
  },

  // Borders
  border: {
    light: '#d0d0d0',
    medium: '#a0a0a0',
    dark: '#707070',
  },

  // Background
  background: '#ffffff',
};
