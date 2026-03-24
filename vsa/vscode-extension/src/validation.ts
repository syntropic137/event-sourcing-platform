import * as vscode from 'vscode';
import * as path from 'path';
import { spawn } from 'child_process';

export interface ValidationError {
    file: string;
    line?: number;
    column?: number;
    message: string;
    severity: 'error' | 'warning';
    code?: string;
}

export interface ValidationReport {
    valid: number;
    errors: ValidationError[];
    warnings: ValidationError[];
}

export class ValidationService {
    private vsaCliPath: string | undefined;

    constructor() {
        this.findVsaCli();
    }

    private findVsaCli(): void {
        // Try to find vsa CLI in PATH
        // In production, we might bundle the CLI or provide installation instructions
        this.vsaCliPath = 'vsa'; // Assumes vsa is in PATH
    }

    async validate(workspacePath: string, configPath: string): Promise<ValidationReport> {
        if (!this.vsaCliPath) {
            throw new Error('VSA CLI not found. Please install vsa CLI tool.');
        }

        return new Promise((resolve, reject) => {
            const args = ['validate', '--config', configPath];
            const vsaProcess = spawn(this.vsaCliPath!, args, {
                cwd: workspacePath,
                stdio: ['ignore', 'pipe', 'pipe']
            });

            let stdout = '';
            let stderr = '';

            vsaProcess.stdout.on('data', (data) => {
                stdout += data.toString();
            });

            vsaProcess.stderr.on('data', (data) => {
                stderr += data.toString();
            });

            vsaProcess.on('close', (code) => {
                if (code === 0 || code === 1) {
                    // Exit code 1 means validation errors found
                    try {
                        const report = this.parseOutput(stdout);
                        resolve(report);
                    } catch (error) {
                        reject(new Error(`Failed to parse validation output: ${error}`));
                    }
                } else {
                    reject(new Error(`VSA CLI exited with code ${code}: ${stderr}`));
                }
            });

            vsaProcess.on('error', (error) => {
                reject(new Error(`Failed to run VSA CLI: ${error.message}`));
            });
        });
    }

    private classifyLine(
        line: string,
        currentFile: string | undefined
    ): { valid?: boolean; error?: ValidationError; newContext?: string } {
        const contextMatch = line.match(/^(✅|❌)\s+(.+)$/);
        if (contextMatch) {
            return { newContext: contextMatch[2].trim(), valid: line.includes('✅') || undefined };
        }
        if (line.includes('❌') || line.includes('⚠️')) {
            return { error: this.parseErrorLine(line, currentFile) ?? undefined };
        }
        return {};
    }

    private parseOutput(output: string): ValidationReport {
        const report: ValidationReport = { valid: 0, errors: [], warnings: [] };
        let currentFile: string | undefined;

        for (const line of output.split('\n')) {
            const classified = this.classifyLine(line, currentFile);
            if (classified.newContext) currentFile = classified.newContext;
            if (classified.valid) report.valid++;
            if (classified.error) {
                (classified.error.severity === 'error' ? report.errors : report.warnings).push(classified.error);
            }
        }
        return report;
    }

    private parseErrorLine(line: string, context?: string): ValidationError | null {
        // Simple parsing for now
        // Format: "   └─ ⚠️  Missing: UpdateProduct.test.ts"
        
        const missingMatch = line.match(/Missing:\s+(.+?)$/);
        if (missingMatch && context) {
            return {
                file: context,
                message: `Missing file: ${missingMatch[1].trim()}`,
                severity: line.includes('❌') ? 'error' : 'warning'
            };
        }

        // Format: "❌ Naming violation: expected CreateProductCommand.ts"
        const namingMatch = line.match(/Naming violation:\s+(.+)$/);
        if (namingMatch && context) {
            return {
                file: context,
                message: `Naming violation: ${namingMatch[1].trim()}`,
                severity: 'error'
            };
        }

        // Generic error/warning
        if (line.includes('❌') || line.includes('⚠️')) {
            return {
                file: context || 'unknown',
                message: line.replace(/^.*?(❌|⚠️)\s+/, '').trim(),
                severity: line.includes('❌') ? 'error' : 'warning'
            };
        }

        return null;
    }

    async hasVsaConfig(workspacePath: string): Promise<string | null> {
        const config = vscode.workspace.getConfiguration('vsa');
        const configPath = config.get<string>('configPath', 'vsa.yaml');
        
        const yamlPath = path.join(workspacePath, configPath);
        const ymlPath = yamlPath.replace('.yaml', '.yml');

        try {
            await vscode.workspace.fs.stat(vscode.Uri.file(yamlPath));
            return yamlPath;
        } catch {
            try {
                await vscode.workspace.fs.stat(vscode.Uri.file(ymlPath));
                return ymlPath;
            } catch {
                return null;
            }
        }
    }
}

