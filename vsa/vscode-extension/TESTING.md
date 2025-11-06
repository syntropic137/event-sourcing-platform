# Testing the VSA VS Code Extension

## Setup

1. Install dependencies:
```bash
cd vsa/vscode-extension
npm install
```

2. Compile the extension:
```bash
npm run compile
```

## Manual Testing

### Test in VS Code

1. Open the `vsa/vscode-extension` folder in VS Code
2. Press `F5` to launch the Extension Development Host
3. A new VS Code window will open with the extension loaded

### Test Scenarios

#### Scenario 1: Extension Activation

1. Create a test workspace with a `vsa.yaml` file
2. Open the workspace in the Extension Development Host
3. Check the Output panel (View → Output → VSA) for activation message
4. Verify the "VSA" status bar item appears

**Expected:** 
- ✅ Extension activates automatically
- ✅ Status bar shows "$(check) VSA"
- ✅ No errors in the Output panel

#### Scenario 2: Validation on Open

1. Open a workspace with a `vsa.yaml` and some vertical slices
2. Wait for validation to complete
3. Check for diagnostics in the Problems panel

**Expected:**
- ✅ Validation runs automatically
- ✅ Errors and warnings show in Problems panel
- ✅ Summary notification appears

#### Scenario 3: Validation on Save

1. Edit a file in a vertical slice (e.g., rename a command file)
2. Save the file (Cmd/Ctrl + S)
3. Check for updated diagnostics

**Expected:**
- ✅ Validation runs on save
- ✅ Diagnostics update in Problems panel
- ✅ New violations appear (if any)

#### Scenario 4: Manual Validation Command

1. Open Command Palette (Cmd/Ctrl + Shift + P)
2. Type "VSA: Validate Architecture"
3. Press Enter

**Expected:**
- ✅ Validation runs with progress indicator
- ✅ Summary notification shows results
- ✅ Diagnostics update

#### Scenario 5: Generate Feature Command

1. Open Command Palette
2. Type "VSA: Generate Feature"
3. Enter a feature path (e.g., `warehouse/products/create-product`)
4. Check that files are created

**Expected:**
- ✅ Prompt for feature path appears
- ✅ Feature scaffolding is created
- ✅ Success notification appears
- ✅ Files are opened in the editor

#### Scenario 6: Quick Fixes

1. Create a validation error (e.g., rename a command file incorrectly)
2. Wait for diagnostics to appear
3. Click on the error in the editor
4. Look for the lightbulb icon
5. Click to see quick fixes

**Expected:**
- ✅ Lightbulb icon appears
- ✅ Quick fixes are suggested (e.g., "Rename to follow naming convention")
- ✅ Applying the fix resolves the issue

#### Scenario 7: YAML Auto-completion

1. Open `vsa.yaml`
2. Start typing a property name
3. Check for auto-completion suggestions

**Expected:**
- ✅ Auto-completion works
- ✅ Schema validation highlights errors
- ✅ Hover shows documentation

#### Scenario 8: Config File Watcher

1. Open a workspace with VSA
2. Edit `vsa.yaml` (e.g., change a bounded context name)
3. Save the file

**Expected:**
- ✅ Extension detects config change
- ✅ Notification shows "VSA config changed, re-validating..."
- ✅ Validation runs automatically

#### Scenario 9: List Features Command

1. Open Command Palette
2. Type "VSA: List Features"
3. Press Enter

**Expected:**
- ✅ New document opens with feature list
- ✅ Features are grouped by context
- ✅ Tree view is properly formatted

#### Scenario 10: Generate Manifest Command

1. Open Command Palette
2. Type "VSA: Generate Manifest"
3. Press Enter

**Expected:**
- ✅ Manifest is generated
- ✅ JSON document opens
- ✅ Success notification appears

## Automated Testing

### Unit Tests

(To be implemented)

```bash
npm run test
```

### Integration Tests

(To be implemented)

```bash
npm run test:integration
```

## Known Limitations

1. **CLI Dependency:** Extension requires `vsa` CLI to be installed and in PATH
2. **Output Parsing:** Uses text parsing instead of JSON (CLI should add `--json` flag)
3. **No LSP:** Uses CLI spawning instead of Language Server Protocol
4. **File Watching:** May have slight delay due to debouncing

## Future Improvements

1. Add comprehensive automated tests
2. Implement Language Server Protocol (LSP) for better performance
3. Add integration tests with real VSA projects
4. Add snapshot tests for code generation
5. Improve error handling and user feedback
6. Add telemetry for usage tracking

