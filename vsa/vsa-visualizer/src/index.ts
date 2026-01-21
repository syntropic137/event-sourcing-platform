#!/usr/bin/env node

import { Command } from 'commander';
import * as fs from 'fs';
import { parseManifest, hasDomainData } from './manifest/parser';
import { ManifestValidationError } from './types/manifest';

const program = new Command();

program
  .name('vsa-visualizer')
  .description('Generate architecture diagrams from VSA manifest')
  .version('0.1.0')
  .argument('[manifest]', 'Path to manifest JSON file or - for stdin', '-')
  .option('-o, --output <dir>', 'Output directory', 'docs/architecture')
  .option('-v, --verbose', 'Enable verbose logging', false)
  .action(async (manifestPath: string, options: { output: string; verbose: boolean }) => {
    try {
      if (options.verbose) {
        console.log('[vsa-visualizer] Starting...');
        console.log(`[vsa-visualizer] Manifest path: ${manifestPath}`);
        console.log(`[vsa-visualizer] Output directory: ${options.output}`);
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
      if (!hasDomainData(manifest)) {
        console.error('Error: Manifest does not contain domain data.');
        console.error('Please regenerate the manifest with: vsa manifest --include-domain');
        process.exit(1);
      }

      if (options.verbose) {
        console.log(`[vsa-visualizer] Found ${manifest.domain!.aggregates.length} aggregates`);
        console.log(`[vsa-visualizer] Found ${manifest.domain!.commands.length} commands`);
        console.log(`[vsa-visualizer] Found ${manifest.domain!.events.length} events`);
      }

      // TODO: Generate diagrams (Milestone 3-5)
      console.log('\n✅ Manifest parsed successfully!');
      console.log('\n📊 Summary:');
      console.log(`   Aggregates: ${manifest.domain!.aggregates.length}`);
      console.log(`   Commands:   ${manifest.domain!.commands.length}`);
      console.log(`   Events:     ${manifest.domain!.events.length}`);

      if (manifest.domain!.queries && manifest.domain!.queries.length > 0) {
        console.log(`   Queries:    ${manifest.domain!.queries.length}`);
      }

      if (manifest.bounded_contexts && manifest.bounded_contexts.length > 0) {
        console.log(`   Bounded Contexts: ${manifest.bounded_contexts.length}`);
      }

      console.log('\n⚠️  Note: Diagram generation not yet implemented (coming in Milestone 3-5)');
      console.log(`    Output will be written to: ${options.output}`);
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
  });

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
