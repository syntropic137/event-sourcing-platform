import * as vscode from 'vscode';
import { VsaDiagnosticProvider } from './diagnostics';
import { registerCommands } from './commands';
import { ValidationService } from './validation';
import { VsaCodeActionProvider, registerCodeActionCommands } from './codeActions';

let diagnosticProvider: VsaDiagnosticProvider | undefined;
let validationService: ValidationService | undefined;

export function activate(context: vscode.ExtensionContext) {
    console.log('VSA extension is now active');

    // Initialize services
    validationService = new ValidationService();
    diagnosticProvider = new VsaDiagnosticProvider(validationService);

    // Register commands
    registerCommands(context, validationService, diagnosticProvider);
    registerCodeActionCommands(context);

    // Register code actions provider
    context.subscriptions.push(
        vscode.languages.registerCodeActionsProvider(
            ['typescript', 'typescriptreact', 'python', 'rust'],
            new VsaCodeActionProvider(),
            {
                providedCodeActionKinds: VsaCodeActionProvider.providedCodeActionKinds
            }
        )
    );

    // Setup file watchers
    setupFileWatchers(context, diagnosticProvider);

    // Run initial validation if configured
    const config = vscode.workspace.getConfiguration('vsa');
    if (config.get<boolean>('validateOnOpen', true)) {
        validateWorkspace(diagnosticProvider);
    }

    // Status bar item
    const statusBarItem = vscode.window.createStatusBarItem(
        vscode.StatusBarAlignment.Left,
        100
    );
    statusBarItem.text = "$(check) VSA";
    statusBarItem.tooltip = "Vertical Slice Architecture - Click to validate";
    statusBarItem.command = 'vsa.validate';
    statusBarItem.show();
    context.subscriptions.push(statusBarItem);

    vscode.window.showInformationMessage('VSA: Extension activated');
}

function setupFileWatchers(
    context: vscode.ExtensionContext,
    diagnosticProvider: VsaDiagnosticProvider
) {
    // Watch for file saves
    const config = vscode.workspace.getConfiguration('vsa');
    if (config.get<boolean>('validateOnSave', true)) {
        context.subscriptions.push(
            vscode.workspace.onDidSaveTextDocument((document) => {
                // Only validate relevant file types
                if (isRelevantFile(document.fileName)) {
                    validateWorkspace(diagnosticProvider);
                }
            })
        );
    }

    // Watch for config file changes
    const configWatcher = vscode.workspace.createFileSystemWatcher(
        '**/vsa.{yaml,yml}'
    );
    
    configWatcher.onDidChange(() => {
        vscode.window.showInformationMessage('VSA config changed, re-validating...');
        validateWorkspace(diagnosticProvider);
    });

    configWatcher.onDidCreate(() => {
        vscode.window.showInformationMessage('VSA config created, validating...');
        validateWorkspace(diagnosticProvider);
    });

    context.subscriptions.push(configWatcher);
}

function isRelevantFile(fileName: string): boolean {
    return (
        fileName.endsWith('.ts') ||
        fileName.endsWith('.tsx') ||
        fileName.endsWith('.py') ||
        fileName.endsWith('.rs') ||
        fileName.endsWith('vsa.yaml') ||
        fileName.endsWith('vsa.yml')
    );
}

async function validateWorkspace(diagnosticProvider: VsaDiagnosticProvider) {
    if (!vscode.workspace.workspaceFolders) {
        return;
    }

    try {
        await diagnosticProvider.validateAndUpdateDiagnostics();
    } catch (error) {
        vscode.window.showErrorMessage(
            `VSA validation failed: ${error instanceof Error ? error.message : String(error)}`
        );
    }
}

export function deactivate() {
    diagnosticProvider?.dispose();
}

