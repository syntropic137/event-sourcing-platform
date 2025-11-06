import * as vscode from 'vscode';
import { ValidationService, ValidationError } from './validation';

export class VsaDiagnosticProvider {
    private diagnosticCollection: vscode.DiagnosticCollection;
    private validationService: ValidationService;

    constructor(validationService: ValidationService) {
        this.validationService = validationService;
        this.diagnosticCollection = vscode.languages.createDiagnosticCollection('vsa');
    }

    async validateAndUpdateDiagnostics(): Promise<void> {
        const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
        if (!workspaceFolder) {
            return;
        }

        const workspacePath = workspaceFolder.uri.fsPath;
        const configPath = await this.validationService.hasVsaConfig(workspacePath);

        if (!configPath) {
            vscode.window.showWarningMessage(
                'No vsa.yaml found in workspace. Run "VSA: Initialize" to create one.'
            );
            return;
        }

        try {
            const report = await this.validationService.validate(workspacePath, configPath);
            this.updateDiagnostics(report.errors.concat(report.warnings), workspacePath);

            // Show summary
            const totalIssues = report.errors.length + report.warnings.length;
            if (totalIssues === 0) {
                vscode.window.showInformationMessage(
                    `VSA: ✅ All ${report.valid} features valid`
                );
            } else {
                const message = `VSA: ${report.errors.length} error(s), ${report.warnings.length} warning(s)`;
                if (report.errors.length > 0) {
                    vscode.window.showErrorMessage(message);
                } else {
                    vscode.window.showWarningMessage(message);
                }
            }
        } catch (error) {
            throw error;
        }
    }

    private updateDiagnostics(errors: ValidationError[], workspacePath: string): void {
        // Clear existing diagnostics
        this.diagnosticCollection.clear();

        // Group errors by file
        const errorsByFile = new Map<string, ValidationError[]>();
        for (const error of errors) {
            const fileErrors = errorsByFile.get(error.file) || [];
            fileErrors.push(error);
            errorsByFile.set(error.file, fileErrors);
        }

        // Create diagnostics for each file
        for (const [file, fileErrors] of errorsByFile.entries()) {
            const uri = this.resolveFileUri(file, workspacePath);
            const diagnostics = fileErrors.map(error => this.createDiagnostic(error));
            this.diagnosticCollection.set(uri, diagnostics);
        }
    }

    private resolveFileUri(file: string, workspacePath: string): vscode.Uri {
        // Convert relative path to absolute URI
        const fullPath = file.startsWith('/') ? file : `${workspacePath}/${file}`;
        return vscode.Uri.file(fullPath);
    }

    private createDiagnostic(error: ValidationError): vscode.Diagnostic {
        // Create range (line and column if available, otherwise entire file)
        const line = error.line ? error.line - 1 : 0;
        const column = error.column ? error.column - 1 : 0;
        const range = new vscode.Range(
            line,
            column,
            line,
            column + 100 // Arbitrary length
        );

        const severity = error.severity === 'error'
            ? vscode.DiagnosticSeverity.Error
            : vscode.DiagnosticSeverity.Warning;

        const diagnostic = new vscode.Diagnostic(
            range,
            error.message,
            severity
        );

        diagnostic.source = 'vsa';
        if (error.code) {
            diagnostic.code = error.code;
        }

        return diagnostic;
    }

    dispose(): void {
        this.diagnosticCollection.dispose();
    }
}

