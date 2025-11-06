import * as vscode from 'vscode';

export class VsaCodeActionProvider implements vscode.CodeActionProvider {
    public static readonly providedCodeActionKinds = [
        vscode.CodeActionKind.QuickFix
    ];

    provideCodeActions(
        document: vscode.TextDocument,
        range: vscode.Range | vscode.Selection,
        context: vscode.CodeActionContext,
        _token: vscode.CancellationToken
    ): vscode.CodeAction[] {
        const actions: vscode.CodeAction[] = [];

        // Check for VSA diagnostics in the current range
        const vsaDiagnostics = context.diagnostics.filter(
            diag => diag.source === 'vsa'
        );

        for (const diagnostic of vsaDiagnostics) {
            // Quick fix for missing files
            if (diagnostic.message.includes('Missing file:')) {
                actions.push(this.createMissingFileAction(diagnostic, document));
            }

            // Quick fix for naming violations
            if (diagnostic.message.includes('Naming violation')) {
                actions.push(this.createRenameFileAction(diagnostic, document));
            }

            // Quick fix for missing tests
            if (diagnostic.message.includes('test')) {
                actions.push(this.createGenerateTestAction(diagnostic, document));
            }
        }

        return actions;
    }

    private createMissingFileAction(
        diagnostic: vscode.Diagnostic,
        document: vscode.TextDocument
    ): vscode.CodeAction {
        const action = new vscode.CodeAction(
            'Create missing file',
            vscode.CodeActionKind.QuickFix
        );
        action.diagnostics = [diagnostic];
        action.isPreferred = true;

        // Extract filename from diagnostic message
        const match = diagnostic.message.match(/Missing file:\s+(.+)/);
        if (match) {
            const filename = match[1].trim();
            action.command = {
                title: 'Create missing file',
                command: 'vsa.createMissingFile',
                arguments: [document.uri, filename]
            };
        }

        return action;
    }

    private createRenameFileAction(
        diagnostic: vscode.Diagnostic,
        document: vscode.TextDocument
    ): vscode.CodeAction {
        const action = new vscode.CodeAction(
            'Rename to follow naming convention',
            vscode.CodeActionKind.QuickFix
        );
        action.diagnostics = [diagnostic];
        action.isPreferred = false;

        // Extract expected filename from diagnostic
        const match = diagnostic.message.match(/expected (.+)/i);
        if (match) {
            const expectedName = match[1].trim();
            action.command = {
                title: 'Rename file',
                command: 'vsa.renameFile',
                arguments: [document.uri, expectedName]
            };
        }

        return action;
    }

    private createGenerateTestAction(
        diagnostic: vscode.Diagnostic,
        document: vscode.TextDocument
    ): vscode.CodeAction {
        const action = new vscode.CodeAction(
            'Generate test file',
            vscode.CodeActionKind.QuickFix
        );
        action.diagnostics = [diagnostic];
        action.isPreferred = true;

        action.command = {
            title: 'Generate test file',
            command: 'vsa.generateTest',
            arguments: [document.uri]
        };

        return action;
    }
}

export function registerCodeActionCommands(context: vscode.ExtensionContext) {
    // Command: Create missing file
    context.subscriptions.push(
        vscode.commands.registerCommand(
            'vsa.createMissingFile',
            async (uri: vscode.Uri, filename: string) => {
                const dirPath = uri.fsPath.substring(0, uri.fsPath.lastIndexOf('/'));
                const newFilePath = `${dirPath}/${filename}`;
                const newUri = vscode.Uri.file(newFilePath);

                try {
                    // Create empty file
                    await vscode.workspace.fs.writeFile(
                        newUri,
                        Buffer.from(getFileTemplate(filename), 'utf-8')
                    );

                    // Open the new file
                    const doc = await vscode.workspace.openTextDocument(newUri);
                    await vscode.window.showTextDocument(doc);

                    vscode.window.showInformationMessage(`Created ${filename}`);
                } catch (error) {
                    vscode.window.showErrorMessage(
                        `Failed to create file: ${error instanceof Error ? error.message : String(error)}`
                    );
                }
            }
        )
    );

    // Command: Rename file
    context.subscriptions.push(
        vscode.commands.registerCommand(
            'vsa.renameFile',
            async (uri: vscode.Uri, expectedName: string) => {
                const dirPath = uri.fsPath.substring(0, uri.fsPath.lastIndexOf('/'));
                const newPath = `${dirPath}/${expectedName}`;
                const newUri = vscode.Uri.file(newPath);

                try {
                    await vscode.workspace.fs.rename(uri, newUri);
                    vscode.window.showInformationMessage(`Renamed to ${expectedName}`);
                } catch (error) {
                    vscode.window.showErrorMessage(
                        `Failed to rename file: ${error instanceof Error ? error.message : String(error)}`
                    );
                }
            }
        )
    );

    // Command: Generate test
    context.subscriptions.push(
        vscode.commands.registerCommand(
            'vsa.generateTest',
            async (uri: vscode.Uri) => {
                const filename = uri.fsPath.substring(uri.fsPath.lastIndexOf('/') + 1);
                const dirPath = uri.fsPath.substring(0, uri.fsPath.lastIndexOf('/'));
                
                // Extract operation name from filename
                const match = filename.match(/^(.+?)(Command|Handler|Event)/);
                if (!match) {
                    vscode.window.showErrorMessage('Could not determine operation name');
                    return;
                }

                const operationName = match[1];
                const extension = filename.substring(filename.lastIndexOf('.'));
                const testFilename = `${operationName}.test${extension}`;
                const testPath = `${dirPath}/${testFilename}`;
                const testUri = vscode.Uri.file(testPath);

                try {
                    await vscode.workspace.fs.writeFile(
                        testUri,
                        Buffer.from(getTestTemplate(operationName, extension), 'utf-8')
                    );

                    const doc = await vscode.workspace.openTextDocument(testUri);
                    await vscode.window.showTextDocument(doc);

                    vscode.window.showInformationMessage(`Created ${testFilename}`);
                } catch (error) {
                    vscode.window.showErrorMessage(
                        `Failed to create test: ${error instanceof Error ? error.message : String(error)}`
                    );
                }
            }
        )
    );
}

function getFileTemplate(filename: string): string {
    // Basic templates for different file types
    if (filename.includes('Command')) {
        return `export interface ${filename.replace('.ts', '').replace('.tsx', '')} {
  // Add command properties here
}
`;
    }

    if (filename.includes('Event')) {
        return `export interface ${filename.replace('.ts', '').replace('.tsx', '')} {
  // Add event properties here
}
`;
    }

    if (filename.includes('Handler')) {
        const className = filename.replace('.ts', '').replace('.tsx', '');
        return `export class ${className} {
  async handle(command: any): Promise<void> {
    // Implement handler logic here
  }
}
`;
    }

    // Default empty file
    return '';
}

function getTestTemplate(operationName: string, extension: string): string {
    if (extension === '.ts' || extension === '.tsx') {
        return `import { describe, it, expect } from '@jest/globals';

describe('${operationName}', () => {
  it('should handle the command successfully', async () => {
    // Arrange
    
    // Act
    
    // Assert
    expect(true).toBe(true);
  });

  it('should handle validation errors', async () => {
    // Arrange
    
    // Act
    
    // Assert
    expect(true).toBe(true);
  });
});
`;
    }

    if (extension === '.py') {
        return `import pytest

class Test${operationName}:
    def test_handle_command_successfully(self):
        # Arrange
        
        # Act
        
        # Assert
        assert True

    def test_handle_validation_errors(self):
        # Arrange
        
        # Act
        
        # Assert
        assert True
`;
    }

    return '// Add tests here\n';
}

