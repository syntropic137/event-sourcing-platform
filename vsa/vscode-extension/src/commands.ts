import * as vscode from 'vscode';
import { ValidationService } from './validation';
import { VsaDiagnosticProvider } from './diagnostics';
import { spawn } from 'child_process';
import * as path from 'path';

export function registerCommands(
    context: vscode.ExtensionContext,
    validationService: ValidationService,
    diagnosticProvider: VsaDiagnosticProvider
) {
    // Command: vsa.validate
    context.subscriptions.push(
        vscode.commands.registerCommand('vsa.validate', async () => {
            await validateCommand(diagnosticProvider);
        })
    );

    // Command: vsa.generateFeature
    context.subscriptions.push(
        vscode.commands.registerCommand('vsa.generateFeature', async () => {
            await generateFeatureCommand();
        })
    );

    // Command: vsa.listFeatures
    context.subscriptions.push(
        vscode.commands.registerCommand('vsa.listFeatures', async () => {
            await listFeaturesCommand();
        })
    );

    // Command: vsa.generateManifest
    context.subscriptions.push(
        vscode.commands.registerCommand('vsa.generateManifest', async () => {
            await generateManifestCommand();
        })
    );
}

async function validateCommand(diagnosticProvider: VsaDiagnosticProvider) {
    await vscode.window.withProgress(
        {
            location: vscode.ProgressLocation.Notification,
            title: "Validating VSA structure...",
            cancellable: false
        },
        async () => {
            try {
                await diagnosticProvider.validateAndUpdateDiagnostics();
            } catch (error) {
                vscode.window.showErrorMessage(
                    `Validation failed: ${error instanceof Error ? error.message : String(error)}`
                );
            }
        }
    );
}

async function generateFeatureCommand() {
    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (!workspaceFolder) {
        vscode.window.showErrorMessage('No workspace folder open');
        return;
    }

    // Prompt for feature path
    const featurePath = await vscode.window.showInputBox({
        prompt: 'Enter feature path (e.g., warehouse/products/create-product)',
        placeHolder: 'context/feature-area/operation',
        validateInput: (value) => {
            if (!value || value.trim().length === 0) {
                return 'Feature path cannot be empty';
            }
            if (!/^[a-z0-9\-\/]+$/.test(value)) {
                return 'Feature path must contain only lowercase letters, numbers, hyphens, and slashes';
            }
            return null;
        }
    });

    if (!featurePath) {
        return;
    }

    // Split path into context and feature
    const parts = featurePath.split('/');
    if (parts.length < 2) {
        vscode.window.showErrorMessage('Feature path must include context and feature name');
        return;
    }

    const context = parts[0];
    const feature = parts.slice(1).join('/');

    // Run vsa generate command
    await vscode.window.withProgress(
        {
            location: vscode.ProgressLocation.Notification,
            title: `Generating feature ${featurePath}...`,
            cancellable: false
        },
        async () => {
            try {
                await runVsaCommand(
                    workspaceFolder.uri.fsPath,
                    ['generate', context, feature, '--interactive']
                );
                vscode.window.showInformationMessage(
                    `✅ Feature ${featurePath} generated successfully`
                );
            } catch (error) {
                vscode.window.showErrorMessage(
                    `Failed to generate feature: ${error instanceof Error ? error.message : String(error)}`
                );
            }
        }
    );
}

async function listFeaturesCommand() {
    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (!workspaceFolder) {
        vscode.window.showErrorMessage('No workspace folder open');
        return;
    }

    try {
        const output = await runVsaCommand(workspaceFolder.uri.fsPath, ['list']);
        
        // Show output in new document
        const doc = await vscode.workspace.openTextDocument({
            content: output,
            language: 'plaintext'
        });
        await vscode.window.showTextDocument(doc);
    } catch (error) {
        vscode.window.showErrorMessage(
            `Failed to list features: ${error instanceof Error ? error.message : String(error)}`
        );
    }
}

async function generateManifestCommand() {
    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (!workspaceFolder) {
        vscode.window.showErrorMessage('No workspace folder open');
        return;
    }

    await vscode.window.withProgress(
        {
            location: vscode.ProgressLocation.Notification,
            title: "Generating manifest...",
            cancellable: false
        },
        async () => {
            try {
                const output = await runVsaCommand(workspaceFolder.uri.fsPath, ['manifest']);
                
                // Parse JSON and show in new document
                const manifestPath = path.join(workspaceFolder.uri.fsPath, 'vsa-manifest.json');
                const uri = vscode.Uri.file(manifestPath);
                
                const doc = await vscode.workspace.openTextDocument({
                    content: output,
                    language: 'json'
                });
                await vscode.window.showTextDocument(doc);
                
                vscode.window.showInformationMessage('✅ Manifest generated successfully');
            } catch (error) {
                vscode.window.showErrorMessage(
                    `Failed to generate manifest: ${error instanceof Error ? error.message : String(error)}`
                );
            }
        }
    );
}

function runVsaCommand(cwd: string, args: string[]): Promise<string> {
    return new Promise((resolve, reject) => {
        const vsaProcess = spawn('vsa', args, {
            cwd,
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
            if (code === 0) {
                resolve(stdout);
            } else {
                reject(new Error(`VSA CLI exited with code ${code}: ${stderr}`));
            }
        });

        vsaProcess.on('error', (error) => {
            reject(new Error(`Failed to run VSA CLI: ${error.message}`));
        });
    });
}

