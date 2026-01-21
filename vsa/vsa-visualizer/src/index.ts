#!/usr/bin/env node

import { Command } from 'commander';
import * as fs from 'fs';
import * as path from 'path';
import { parseManifest, hasDomainData } from './manifest/parser';
import { ManifestValidationError } from './types/manifest';
import { OverviewGenerator } from './generators/overview-generator';
import { AggregateGenerator } from './generators/aggregate-generator';
import { FlowsGenerator } from './generators/flows-generator';
import { writeFile, writeFiles, ensureDirectoryExists } from './utils/file-writer';

const program = new Command();

program
  .name('vsa-visualizer')
  .description('Generate architecture diagrams from VSA manifest')
  .version('0.1.0')
  .argument('[manifest]', 'Path to manifest JSON file or - for stdin', '-')
  .option('-o, --output <dir>', 'Output directory', 'docs/architecture')
  .option('-f, --format <type>', 'Output format (currently only "mermaid")', 'mermaid')
  .option('-v, --verbose', 'Enable verbose logging', false)
  .addHelpText(
    'after',
    `
Examples:
  $ vsa-visualizer manifest.json
  $ vsa-visualizer manifest.json --output ./docs/arch
  $ vsa manifest --include-domain | vsa-visualizer
  $ vsa-visualizer --help

For more information, see: https://github.com/your-org/vsa
`
  )
  .action(
    async (manifestPath: string, options: { output: string; format: string; verbose: boolean }) => {
      try {
        // Validate format
        if (options.format !== 'mermaid') {
          console.error(
            `Error: Unsupported format "${options.format}". Currently only "mermaid" is supported.`
          );
          process.exit(1);
        }

        if (options.verbose) {
          console.log('[vsa-visualizer] Starting...');
          console.log(`[vsa-visualizer] Manifest path: ${manifestPath}`);
          console.log(`[vsa-visualizer] Output directory: ${options.output}`);
          console.log(`[vsa-visualizer] Output format: ${options.format}`);
        }

        // Read manifest
        let manifestContent: string;

        if (manifestPath === '-') {
          // Read from stdin
          if (options.verbose) {
            console.log('[vsa-visualizer] Reading manifest from stdin...');
          }
          manifestContent = await readStdin();
        } else {
          // Read from file
          if (!fs.existsSync(manifestPath)) {
            console.error(`Error: Manifest file not found: ${manifestPath}`);
            process.exit(1);
          }
          if (options.verbose) {
            console.log(`[vsa-visualizer] Reading manifest from file: ${manifestPath}`);
          }
          manifestContent = fs.readFileSync(manifestPath, 'utf-8');
        }

        // Parse and validate manifest
        if (options.verbose) {
          console.log('[vsa-visualizer] Parsing manifest...');
        }

        const manifest = parseManifest(manifestContent);

        if (options.verbose) {
          console.log(`[vsa-visualizer] Manifest schema version: ${manifest.schema_version}`);
          console.log(`[vsa-visualizer] VSA version: ${manifest.version}`);
        }

        // Check if domain data is present
        const hasAggregates = hasDomainData(manifest);
        
        if (!hasAggregates) {
          if (options.verbose) {
            console.log('[vsa-visualizer] Note: No domain model found (aggregates/commands/events).');
            console.log('[vsa-visualizer] Will generate context-level documentation only.');
          }
        } else if (options.verbose) {
          console.log(`[vsa-visualizer] Found ${manifest.domain!.aggregates.length} aggregates`);
          console.log(`[vsa-visualizer] Found ${manifest.domain!.commands.length} commands`);
          console.log(`[vsa-visualizer] Found ${manifest.domain!.events.length} events`);
        }

        // Ensure output directory exists
        ensureDirectoryExists(options.output);

        const generatedFiles: string[] = [];

        // Generate Overview
        if (options.verbose) {
          console.log('[vsa-visualizer] Generating overview...');
        }

        const overviewGenerator = new OverviewGenerator(manifest);
        const overviewContent = overviewGenerator.generate();
        const overviewPath = path.join(options.output, 'OVERVIEW.md');
        writeFile(overviewPath, overviewContent);
        generatedFiles.push(overviewPath);

        // Generate Aggregate pages (only if we have aggregates)
        if (hasAggregates) {
          if (options.verbose) {
            console.log('[vsa-visualizer] Generating aggregate documentation...');
          }

          const aggregateGenerator = new AggregateGenerator(manifest);
          const aggregatePages = aggregateGenerator.generateAll();
          const aggregatePaths = writeFiles(aggregatePages, options.output);
          generatedFiles.push(...aggregatePaths);

          // Generate Flows documentation
          if (options.verbose) {
            console.log('[vsa-visualizer] Detecting cross-aggregate flows...');
          }

          const flowsGenerator = new FlowsGenerator(manifest);
          const flowsContent = flowsGenerator.generate();

          if (flowsContent) {
            const flowsPath = path.join(options.output, 'FLOWS.md');
            writeFile(flowsPath, flowsContent);
            generatedFiles.push(flowsPath);
          } else if (options.verbose) {
            console.log('[vsa-visualizer] No cross-aggregate flows detected');
          }
        } else if (options.verbose) {
          console.log('[vsa-visualizer] Skipping aggregate and flow documentation (no domain data)');
        }

        // Summary
        console.log('\n✅ Documentation generated successfully!');
        console.log('\n📊 Summary:');
        
        if (hasAggregates) {
          console.log(`   Aggregates: ${manifest.domain!.aggregates.length}`);
          console.log(`   Commands:   ${manifest.domain!.commands.length}`);
          console.log(`   Events:     ${manifest.domain!.events.length}`);

          if (manifest.domain!.queries && manifest.domain!.queries.length > 0) {
            console.log(`   Queries:    ${manifest.domain!.queries.length}`);
          }
        } else {
          console.log(`   Contexts:   ${(manifest as any).contexts?.length || 0}`);
          console.log(`   (Domain model not detected)`);
        }

        if (manifest.bounded_contexts && manifest.bounded_contexts.length > 0) {
          console.log(`   Bounded Contexts: ${manifest.bounded_contexts.length}`);
        }

        console.log('\n📝 Generated files:');
        for (const filePath of generatedFiles) {
          console.log(`   - ${filePath}`);
        }
      } catch (error) {
        if (error instanceof ManifestValidationError) {
          console.error(`\nValidation Error: ${error.message}`);
          process.exit(1);
        } else if (error instanceof Error) {
          console.error(`\nError: ${error.message}`);
          if (options.verbose && error.stack) {
            console.error(error.stack);
          }
          process.exit(1);
        } else {
          console.error('\nUnknown error occurred');
          process.exit(1);
        }
      }
    }
  );

/**
 * Read all data from stdin
 */
async function readStdin(): Promise<string> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];

    process.stdin.on('data', (chunk: Buffer) => {
      chunks.push(chunk);
    });

    process.stdin.on('end', () => {
      resolve(Buffer.concat(chunks).toString('utf-8'));
    });

    process.stdin.on('error', (error: Error) => {
      reject(error);
    });
  });
}

program.parse();
