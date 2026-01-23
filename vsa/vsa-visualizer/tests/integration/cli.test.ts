import { execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

describe('CLI Integration Tests', () => {
  let tempDir: string;
  const cliPath = path.join(__dirname, '..', '..', 'dist', 'index.js');
  const fixturePath = path.join(__dirname, '..', 'fixtures', 'test-manifest.json');

  beforeEach(() => {
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'vsa-viz-cli-test-'));
  });

  afterEach(() => {
    if (fs.existsSync(tempDir)) {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  describe('basic usage', () => {
    it('should generate documentation from file', () => {
      const outputDir = path.join(tempDir, 'output');
      execSync(`node ${cliPath} ${fixturePath} --output ${outputDir}`, {
        encoding: 'utf-8',
      });

      expect(fs.existsSync(path.join(outputDir, 'OVERVIEW.md'))).toBe(true);
      expect(fs.existsSync(path.join(outputDir, 'aggregates', 'CartAggregate.md'))).toBe(true);
      expect(fs.existsSync(path.join(outputDir, 'aggregates', 'README.md'))).toBe(true);
    });

    it('should generate documentation from stdin', () => {
      const outputDir = path.join(tempDir, 'output');
      const manifest = fs.readFileSync(fixturePath, 'utf-8');

      execSync(`echo '${manifest}' | node ${cliPath} - --output ${outputDir}`, {
        encoding: 'utf-8',
        shell: '/bin/bash',
      });

      expect(fs.existsSync(path.join(outputDir, 'OVERVIEW.md'))).toBe(true);
    });

    it('should use default output directory when not specified', () => {
      const cwd = tempDir;
      execSync(`node ${cliPath} ${fixturePath}`, {
        cwd,
        encoding: 'utf-8',
      });

      expect(fs.existsSync(path.join(cwd, 'docs', 'architecture', 'OVERVIEW.md'))).toBe(true);
    });
  });

  describe('error handling', () => {
    it('should fail with non-existent file', () => {
      expect(() => {
        execSync(`node ${cliPath} /nonexistent/file.json --output ${tempDir}`, {
          encoding: 'utf-8',
        });
      }).toThrow();
    });

    it('should fail with invalid JSON', () => {
      const invalidFile = path.join(tempDir, 'invalid.json');
      fs.writeFileSync(invalidFile, 'not valid json');

      expect(() => {
        execSync(`node ${cliPath} ${invalidFile} --output ${tempDir}`, {
          encoding: 'utf-8',
        });
      }).toThrow();
    });

    it('should succeed with manifest missing domain data (generates minimal docs)', () => {
      const noDomainFile = path.join(tempDir, 'no-domain.json');
      fs.writeFileSync(
        noDomainFile,
        JSON.stringify({
          version: '0.6.1-beta',
          schema_version: '2.0.0',
          generated_at: '2026-01-21T00:00:00Z',
          bounded_contexts: [
            {
              name: 'TestContext',
              path: '/app/contexts/test',
              features: [],
            },
          ],
        })
      );

      // Should succeed but generate minimal documentation
      const output = execSync(`node ${cliPath} ${noDomainFile} --output ${tempDir}`, {
        encoding: 'utf-8',
      });

      expect(output).toContain('Documentation generated successfully');
    });

    it('should fail with unsupported format', () => {
      expect(() => {
        execSync(`node ${cliPath} ${fixturePath} --output ${tempDir} --format plantuml`, {
          encoding: 'utf-8',
        });
      }).toThrow(/Unsupported format/);
    });
  });

  describe('help and version', () => {
    it('should display help', () => {
      const output = execSync(`node ${cliPath} --help`, {
        encoding: 'utf-8',
      });

      expect(output).toContain('vsa-visualizer');
      expect(output).toContain('Generate architecture diagrams');
      expect(output).toContain('Examples:');
    });

    it('should display version', () => {
      const output = execSync(`node ${cliPath} --version`, {
        encoding: 'utf-8',
      });

      expect(output).toContain('0.1.0');
    });
  });

  describe('verbose mode', () => {
    it('should display verbose output', () => {
      const outputDir = path.join(tempDir, 'output');
      const output = execSync(`node ${cliPath} ${fixturePath} --output ${outputDir} --verbose`, {
        encoding: 'utf-8',
      });

      expect(output).toContain('[vsa-visualizer]');
      expect(output).toContain('Generating overview');
      expect(output).toContain('Generating aggregate documentation');
    });
  });

  describe('generated content', () => {
    it('should generate valid mermaid diagrams', () => {
      const outputDir = path.join(tempDir, 'output');
      execSync(`node ${cliPath} ${fixturePath} --output ${outputDir}`, {
        encoding: 'utf-8',
      });

      const overview = fs.readFileSync(path.join(outputDir, 'OVERVIEW.md'), 'utf-8');
      expect(overview).toContain('```mermaid');
      expect(overview).toContain('C4Context');
      expect(overview).toContain('graph TB');
    });

    it('should include all aggregates', () => {
      const outputDir = path.join(tempDir, 'output');
      execSync(`node ${cliPath} ${fixturePath} --output ${outputDir}`, {
        encoding: 'utf-8',
      });

      const aggregate = fs.readFileSync(
        path.join(outputDir, 'aggregates', 'CartAggregate.md'),
        'utf-8'
      );
      expect(aggregate).toContain('# CartAggregate');
      expect(aggregate).toContain('Commands Handled');
      expect(aggregate).toContain('Events Handled');
    });

    it('should create aggregates index', () => {
      const outputDir = path.join(tempDir, 'output');
      execSync(`node ${cliPath} ${fixturePath} --output ${outputDir}`, {
        encoding: 'utf-8',
      });

      const index = fs.readFileSync(path.join(outputDir, 'aggregates', 'README.md'), 'utf-8');
      expect(index).toContain('# Aggregates');
      expect(index).toContain('CartAggregate');
    });
  });
});
