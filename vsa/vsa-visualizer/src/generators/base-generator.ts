import { Manifest } from '../types/manifest';

/**
 * Base class for all diagram generators
 */
export abstract class BaseGenerator {
  constructor(protected manifest: Manifest) {}

  /**
   * Generate the markdown content for this generator
   * @returns markdown content, or null if nothing to generate
   */
  abstract generate(): string | null;

  /**
   * Get the title for the generated document
   */
  protected abstract getTitle(): string;

  /**
   * Generate a markdown header with title and metadata
   */
  protected generateHeader(): string {
    const title = this.getTitle();
    const generatedDate = new Date().toISOString().split('T')[0];

    return `# ${title}

> **Generated**: ${generatedDate}  
> **VSA Version**: ${this.manifest.version}  
> **Schema Version**: ${this.manifest.schema_version}

---

`;
  }

  /**
   * Wrap mermaid diagram code in markdown code fence
   */
  protected wrapMermaid(diagram: string): string {
    return `\`\`\`mermaid
${diagram}
\`\`\`

`;
  }

  /**
   * Create a section header
   */
  protected section(title: string, level: number = 2): string {
    const hashes = '#'.repeat(level);
    return `${hashes} ${title}\n\n`;
  }

  /**
   * Create a paragraph
   */
  protected paragraph(text: string): string {
    return `${text}\n\n`;
  }

  /**
   * Create a bulleted list
   */
  protected list(items: string[]): string {
    return items.map((item) => `- ${item}`).join('\n') + '\n\n';
  }

  /**
   * Create a table
   */
  protected table(headers: string[], rows: string[][]): string {
    const headerRow = `| ${headers.join(' | ')} |`;
    const separator = `| ${headers.map(() => '---').join(' | ')} |`;
    const dataRows = rows.map((row) => `| ${row.join(' | ')} |`).join('\n');

    return `${headerRow}\n${separator}\n${dataRows}\n\n`;
  }
}
