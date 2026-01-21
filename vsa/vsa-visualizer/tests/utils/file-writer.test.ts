import {
  ensureDirectoryExists,
  writeFile,
  writeFiles,
  fileExists,
  readFile,
} from '../../src/utils/file-writer';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

describe('File Writer Utilities', () => {
  let tempDir: string;

  beforeEach(() => {
    // Create a temporary directory for tests
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'vsa-visualizer-test-'));
  });

  afterEach(() => {
    // Clean up temporary directory
    if (fs.existsSync(tempDir)) {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  describe('ensureDirectoryExists', () => {
    it('should create directory if it does not exist', () => {
      const testDir = path.join(tempDir, 'test-dir');
      expect(fs.existsSync(testDir)).toBe(false);

      ensureDirectoryExists(testDir);

      expect(fs.existsSync(testDir)).toBe(true);
      expect(fs.statSync(testDir).isDirectory()).toBe(true);
    });

    it('should create nested directories', () => {
      const nestedDir = path.join(tempDir, 'level1', 'level2', 'level3');

      ensureDirectoryExists(nestedDir);

      expect(fs.existsSync(nestedDir)).toBe(true);
      expect(fs.statSync(nestedDir).isDirectory()).toBe(true);
    });

    it('should not throw error if directory already exists', () => {
      const testDir = path.join(tempDir, 'existing-dir');
      fs.mkdirSync(testDir);

      expect(() => ensureDirectoryExists(testDir)).not.toThrow();
    });
  });

  describe('writeFile', () => {
    it('should write file with content', () => {
      const filePath = path.join(tempDir, 'test.txt');
      const content = 'Hello, World!';

      writeFile(filePath, content);

      expect(fs.existsSync(filePath)).toBe(true);
      expect(fs.readFileSync(filePath, 'utf-8')).toBe(content);
    });

    it('should create parent directories if they do not exist', () => {
      const filePath = path.join(tempDir, 'nested', 'dir', 'test.txt');
      const content = 'Test content';

      writeFile(filePath, content);

      expect(fs.existsSync(filePath)).toBe(true);
      expect(fs.readFileSync(filePath, 'utf-8')).toBe(content);
    });

    it('should overwrite existing file', () => {
      const filePath = path.join(tempDir, 'test.txt');

      writeFile(filePath, 'First content');
      expect(fs.readFileSync(filePath, 'utf-8')).toBe('First content');

      writeFile(filePath, 'Second content');
      expect(fs.readFileSync(filePath, 'utf-8')).toBe('Second content');
    });
  });

  describe('writeFiles', () => {
    it('should write multiple files', () => {
      const files = new Map<string, string>([
        ['file1.txt', 'Content 1'],
        ['file2.txt', 'Content 2'],
        ['nested/file3.txt', 'Content 3'],
      ]);

      const writtenPaths = writeFiles(files, tempDir);

      expect(writtenPaths).toHaveLength(3);
      expect(fs.readFileSync(path.join(tempDir, 'file1.txt'), 'utf-8')).toBe('Content 1');
      expect(fs.readFileSync(path.join(tempDir, 'file2.txt'), 'utf-8')).toBe('Content 2');
      expect(fs.readFileSync(path.join(tempDir, 'nested', 'file3.txt'), 'utf-8')).toBe('Content 3');
    });

    it('should return array of written file paths', () => {
      const files = new Map<string, string>([
        ['file1.txt', 'Content 1'],
        ['file2.txt', 'Content 2'],
      ]);

      const writtenPaths = writeFiles(files, tempDir);

      expect(writtenPaths).toContain(path.join(tempDir, 'file1.txt'));
      expect(writtenPaths).toContain(path.join(tempDir, 'file2.txt'));
    });
  });

  describe('fileExists', () => {
    it('should return true for existing file', () => {
      const filePath = path.join(tempDir, 'test.txt');
      fs.writeFileSync(filePath, 'content');

      expect(fileExists(filePath)).toBe(true);
    });

    it('should return false for non-existing file', () => {
      const filePath = path.join(tempDir, 'non-existing.txt');

      expect(fileExists(filePath)).toBe(false);
    });

    it('should return true for existing directory', () => {
      expect(fileExists(tempDir)).toBe(true);
    });
  });

  describe('readFile', () => {
    it('should read file content', () => {
      const filePath = path.join(tempDir, 'test.txt');
      const content = 'Test content';
      fs.writeFileSync(filePath, content);

      expect(readFile(filePath)).toBe(content);
    });

    it('should throw error for non-existing file', () => {
      const filePath = path.join(tempDir, 'non-existing.txt');

      expect(() => readFile(filePath)).toThrow();
    });
  });
});
