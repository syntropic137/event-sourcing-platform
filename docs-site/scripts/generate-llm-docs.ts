#!/usr/bin/env node
/**
 * Generate LLM-friendly API documentation
 * 
 * This script concatenates all API reference documentation into a single
 * plain text file optimized for LLM consumption.
 */

import * as fs from 'fs';
import * as path from 'path';

interface DocSection {
  title: string;
  files: string[];
}

const DOCS_DIR = path.join(__dirname, '../docs');
const OUTPUT_FILE = path.join(__dirname, '../static/api-docs-llm.txt');

// Define documentation sections in order
const DOC_SECTIONS: DocSection[] = [
  {
    title: 'EVENT STORE SDK API REFERENCE',
    files: [
      'event-store/sdks/api-reference.md',
      'event-store/sdks/typescript/typescript-sdk.md',
    ]
  },
  {
    title: 'EVENT SOURCING SDK API REFERENCE',
    files: [
      'event-sourcing/sdks/overview/sdk-overview.md',
      'event-sourcing/sdks/api-reference.md',
      'event-sourcing/sdks/typescript/typescript-sdk.md',
      'event-sourcing/sdks/typescript/cqrs-guide.md',
    ]
  },
  {
    title: 'CORE CONCEPTS',
    files: [
      'event-sourcing/aggregates.md',
      'event-sourcing/events.md',
      'event-sourcing/projections.md',
    ]
  }
];

/**
 * Strip markdown formatting and convert to plain text
 */
function markdownToPlainText(markdown: string): string {
  let text = markdown;
  
  // Remove frontmatter
  text = text.replace(/^---\n[\s\S]*?\n---\n/, '');
  
  // Remove HTML comments
  text = text.replace(/<!--[\s\S]*?-->/g, '');
  
  // Convert headers to plain text with section markers
  text = text.replace(/^#{1,6}\s+(.+)$/gm, '\n=== $1 ===\n');
  
  // Remove code block language specifiers but keep content
  text = text.replace(/```(\w+)?\n/g, '\n--- CODE ---\n');
  text = text.replace(/```/g, '\n--- END CODE ---\n');
  
  // Remove inline code backticks
  text = text.replace(/`([^`]+)`/g, '$1');
  
  // Remove bold/italic
  text = text.replace(/\*\*([^*]+)\*\*/g, '$1');
  text = text.replace(/\*([^*]+)\*/g, '$1');
  text = text.replace(/__([^_]+)__/g, '$1');
  text = text.replace(/_([^_]+)_/g, '$1');
  
  // Remove links but keep text
  text = text.replace(/\[([^\]]+)\]\([^)]+\)/g, '$1');
  
  // Remove images
  text = text.replace(/!\[([^\]]*)\]\([^)]+\)/g, '');
  
  // Remove horizontal rules
  text = text.replace(/^[-*_]{3,}$/gm, '');
  
  // Clean up excessive whitespace
  text = text.replace(/\n{3,}/g, '\n\n');
  text = text.trim();
  
  return text;
}

/**
 * Read and process a documentation file
 */
function processDocFile(filePath: string): string | null {
  const fullPath = path.join(DOCS_DIR, filePath);
  
  if (!fs.existsSync(fullPath)) {
    console.warn(`⚠️  File not found: ${filePath}`);
    return null;
  }
  
  try {
    const content = fs.readFileSync(fullPath, 'utf-8');
    return markdownToPlainText(content);
  } catch (error) {
    console.error(`❌ Error reading ${filePath}:`, error);
    return null;
  }
}

/**
 * Generate the complete LLM documentation
 */
function generateLLMDocs(): void {
  console.log('🚀 Generating LLM-friendly API documentation...\n');
  
  const sections: string[] = [];
  
  // Add header
  const header = `
=== EVENT SOURCING PLATFORM API REFERENCE ===
Generated: ${new Date().toISOString()}
Format: Plain Text (LLM-optimized)

This document contains comprehensive API documentation for the Event Sourcing Platform,
formatted for easy consumption by Large Language Models and AI assistants.

Table of Contents:
${DOC_SECTIONS.map((section, i) => `${i + 1}. ${section.title}`).join('\n')}

================================================================================
`.trim();
  
  sections.push(header);
  
  // Process each section
  for (const section of DOC_SECTIONS) {
    console.log(`📄 Processing section: ${section.title}`);
    
    const sectionHeader = `

${'='.repeat(80)}
SECTION: ${section.title}
${'='.repeat(80)}
`;
    
    sections.push(sectionHeader);
    
    for (const file of section.files) {
      console.log(`   - ${file}`);
      const content = processDocFile(file);
      
      if (content) {
        sections.push(`\n--- Document: ${file} ---\n`);
        sections.push(content);
      }
    }
  }
  
  // Add footer
  const footer = `

${'='.repeat(80)}
END OF API REFERENCE
${'='.repeat(80)}

For the latest documentation, visit: https://docs.event-sourcing-platform.dev
For issues and contributions: https://github.com/neurale/event-sourcing-platform
`;
  
  sections.push(footer);
  
  // Write output
  const output = sections.join('\n');
  
  // Ensure static directory exists
  const staticDir = path.dirname(OUTPUT_FILE);
  if (!fs.existsSync(staticDir)) {
    fs.mkdirSync(staticDir, { recursive: true });
  }
  
  fs.writeFileSync(OUTPUT_FILE, output, 'utf-8');
  
  const sizeKB = (output.length / 1024).toFixed(2);
  console.log(`\n✅ Generated: ${OUTPUT_FILE}`);
  console.log(`📊 Size: ${sizeKB} KB`);
  console.log(`📝 Sections: ${DOC_SECTIONS.length}`);
  console.log(`📄 Files processed: ${DOC_SECTIONS.reduce((sum, s) => sum + s.files.length, 0)}`);
}

// Run the generator
try {
  generateLLMDocs();
  process.exit(0);
} catch (error) {
  console.error('❌ Error generating LLM docs:', error);
  process.exit(1);
}
