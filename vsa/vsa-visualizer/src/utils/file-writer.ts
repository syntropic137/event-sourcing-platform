import * as fs from 'fs';
import * as path from 'path';

/**
 * Ensure a directory exists, creating it if necessary
 */
export function ensureDirectoryExists(dirPath: string): void {
  if (!fs.existsSync(dirPath)) {
    fs.mkdirSync(dirPath, { recursive: true });
  }
}

/**
 * Write a file to disk, creating parent directories if needed
 */
export function writeFile(filePath: string, content: string): void {
  const dirPath = path.dirname(filePath);
  ensureDirectoryExists(dirPath);
  fs.writeFileSync(filePath, content, 'utf-8');
}

/**
 * Write multiple files to disk
 * @returns Array of written file paths
 */
export function writeFiles(files: Map<string, string>, baseDir: string): string[] {
  const writtenPaths: string[] = [];

  for (const [relativePath, content] of files.entries()) {
    const fullPath = path.join(baseDir, relativePath);
    writeFile(fullPath, content);
    writtenPaths.push(fullPath);
  }

  return writtenPaths;
}

/**
 * Check if a file exists
 */
export function fileExists(filePath: string): boolean {
  return fs.existsSync(filePath);
}

/**
 * Read a file from disk
 */
export function readFile(filePath: string): string {
  return fs.readFileSync(filePath, 'utf-8');
}

/**
 * Get relative path from base directory
 */
export function getRelativePath(from: string, to: string): string {
  return path.relative(from, to);
}
