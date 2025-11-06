---
sidebar_position: 1
---

# VS Code Extension

Real-time validation and development tools integrated directly into Visual Studio Code.

## Overview

The VSA VS Code extension brings all VSA functionality into your editor with:
- 🔍 Real-time validation
- 💡 Quick fixes
- ⚡ Command palette integration
- 📝 YAML schema auto-completion
- 🎨 Syntax highlighting

## Installation

### Option 1: Manual Installation from VSIX (Recommended)

This method works on all platforms without additional setup:

```bash
# Navigate to extension directory
cd vsa/vscode-extension

# Install dependencies
npm install

# Package extension
npm run package
```

**Install in VS Code:**

1. Open Visual Studio Code
2. Press `Cmd+Shift+X` (Mac) or `Ctrl+Shift+X` (Windows/Linux) to open Extensions
3. Click the `...` menu (three dots) at the top-right of the Extensions panel
4. Select **"Install from VSIX..."**
5. Navigate to `vsa/vscode-extension/vsa-vscode-0.1.0.vsix`
6. Click **Install**
7. Reload VS Code when prompted

### Option 2: Install via CLI

If you have the VS Code CLI (`code` command) set up:

```bash
# From the extension directory
code --install-extension vsa-vscode-0.1.0.vsix
```

**macOS Users:** If you get an error like `open: unrecognized option '--install-extension'`, you need to install the `code` CLI first:

1. Open VS Code
2. Press `Cmd+Shift+P`
3. Type: **"Shell Command: Install 'code' command in PATH"**
4. Select that option
5. Restart your terminal
6. Then run the `code --install-extension` command

**Alternative (macOS):** Use the full path to the code binary:
```bash
/Applications/Visual\ Studio\ Code.app/Contents/Resources/app/bin/code --install-extension vsa-vscode-0.1.0.vsix
```

### Option 3: From VS Code Marketplace

Once published to the marketplace:

1. Open VS Code
2. Press `Cmd+Shift+X` (Extensions)
3. Search for "VSA Manager"
4. Click Install

## Features

### Real-Time Validation

Errors appear as you type:

```typescript
// PlaceOrderHandler.ts

// ❌ Error: Missing OrderPlacedEvent.ts
export class PlaceOrderHandler {
  //              ~~~~~~~~~~~~~~
  // VSA: Handler requires corresponding event file
}
```

**Validation Triggers:**
- On file save
- On file open
- While typing (debounced)

### Inline Diagnostics

See violations directly in your code:

```typescript
// ❌ Boundary Violation
import { GetProductQuery } from '../../../catalog/queries/GetProduct';
//                              ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// VSA: Direct cross-context import not allowed (orders → catalog)
// 💡 Quick Fix: Use integration events instead
```

### Quick Fixes

Press `Cmd+.` (or `Ctrl+.`) on error for fixes:

```typescript
// Before:
❌ Missing file: PlaceOrderHandler.ts

// After quick fix:
✅ Created PlaceOrderHandler.ts with template
```

**Available Quick Fixes:**
- Create missing handler
- Create missing test
- Create missing event
- Rename to follow convention
- Move to correct location
- Remove boundary violation

### Problems Panel

All violations in one place:

```
PROBLEMS (4)
  ❌ VSA
    Missing file: PlaceOrderHandler.ts
      contexts/orders/place-order
    
    Invalid file name: place-order-command.ts
      contexts/orders/place-order/place-order-command.ts
      Expected: PlaceOrderCommand.ts
    
    Boundary violation: Direct cross-context import
      contexts/orders/place-order/PlaceOrderHandler.ts:5
```

### Command Palette

Press `Cmd+Shift+P` for VSA commands:

- **VSA: Validate Project** - Run validation
- **VSA: Generate Feature** - Create new feature
- **VSA: List Features** - Show all features
- **VSA: Show Manifest** - Display architecture
- **VSA: Restart Language Server** - Restart validation

### YAML Auto-Completion

IntelliSense in `vsa.yaml`:

```yaml
version: 1
language: │  # Auto-complete: typescript, python, rust

bounded_contexts:
  - name: orders
    publishes:  # Auto-complete from defined events
      - │  # Suggests: OrderPlaced, OrderCancelled
```

### File Templates

Right-click in explorer:

```
📁 contexts/orders/
  ├─ 📄 place-order/
  │   └─ [Right-click]
  │       └─ VSA: New Command
  │       └─ VSA: New Event  
  │       └─ VSA: New Handler
  │       └─ VSA: New Test
```

## Configuration

### Extension Settings

Access via Settings → Extensions → VSA:

```json
{
  "vsa.enabled": true,
  "vsa.configPath": "vsa.yaml",
  "vsa.validateOnSave": true,
  "vsa.validateOnType": false,
  "vsa.showInlineErrors": true,
  "vsa.quickFixEnabled": true,
  "vsa.logLevel": "info"
}
```

### Per-Workspace Settings

`.vscode/settings.json`:

```json
{
  "vsa.configPath": "./config/vsa.yaml",
  "vsa.validateOnType": true,
  "vsa.logLevel": "debug"
}
```

## Usage

### Starting the Extension

Extension activates automatically when:
- Opening a folder with `vsa.yaml`
- Opening a file in `contexts/` directory
- Running a VSA command

### Validation Workflow

1. **Open file** → Extension validates
2. **Make changes** → Real-time feedback
3. **Save file** → Re-validate
4. **Fix errors** → Use quick fixes

### Generating Features

1. Press `Cmd+Shift+P`
2. Type "VSA: Generate Feature"
3. Enter context name
4. Enter feature name
5. Files created automatically

### Viewing Architecture

1. Press `Cmd+Shift+P`
2. Type "VSA: Show Manifest"
3. See full architecture in new tab

## Language Server

The extension uses a language server for intelligence:

```
VS Code Extension
       │
       ↓
Language Server (Rust)
       │
       ↓
vsa-core (Validation Logic)
```

### Server Status

Check in Output panel:

1. View → Output
2. Select "VSA Language Server"

```
[Info] VSA Language Server started
[Info] Loaded configuration: vsa.yaml
[Info] Found 3 contexts, 12 features
[Info] Validation passed: 0 errors
```

### Restarting Server

If issues occur:

1. Press `Cmd+Shift+P`
2. "VSA: Restart Language Server"
3. Server restarts with fresh state

## Validation Indicators

### In Editor

```typescript
// Green underline: Warning
export class PlaceOrderHandler {
  // ~~~~~~~~~~~~~~~
}

// Red underline: Error
import { X } from '../../other-context';
//                ~~~~~~~~~~~~~~~~~~~~
```

### In Explorer

```
📁 contexts/
  ├─ ✅ orders/          (valid)
  ├─ ❌ inventory/       (errors)
  └─ ⚠️  shipping/       (warnings)
```

### Status Bar

```
VSA: ✅ 12 features, 0 errors
```

Click to run validation.

## Quick Fix Examples

### Create Missing Handler

**Before:**
```
contexts/orders/place-order/
├─ PlaceOrderCommand.ts
└─ ❌ Missing: PlaceOrderHandler.ts
```

**Quick Fix:**
1. Click on error
2. Press `Cmd+.`
3. Select "Create PlaceOrderHandler.ts"

**After:**
```
contexts/orders/place-order/
├─ PlaceOrderCommand.ts
└─ ✅ PlaceOrderHandler.ts (created)
```

### Rename File

**Before:**
```typescript
// place-order-command.ts
❌ Invalid file name
```

**Quick Fix:**
1. Press `Cmd+.` on error
2. Select "Rename to PlaceOrderCommand.ts"

**After:**
```typescript
// PlaceOrderCommand.ts
✅ Follows convention
```

### Fix Boundary Violation

**Before:**
```typescript
import { GetProduct } from '../../../catalog/queries/GetProduct';
❌ Cross-context import
```

**Quick Fix:**
1. Press `Cmd+.`
2. Select "Use integration event instead"
3. Creates subscriber template

## Keyboard Shortcuts

Default shortcuts:

| Action | Shortcut |
|--------|----------|
| Validate | `Cmd+Shift+V` |
| Generate Feature | `Cmd+Shift+G` |
| Show Problems | `Cmd+Shift+M` |
| Quick Fix | `Cmd+.` |
| Go to Definition | `F12` |

Customize in Keyboard Shortcuts settings.

## Integration with CLI

Extension uses CLI under the hood:

```
Extension → Language Server → vsa-core → Validation
                                │
                                └─ Same logic as CLI
```

This ensures consistency between:
- Editor validation
- CLI validation
- CI/CD validation

## Troubleshooting

### Extension Not Working

**Check:**
1. Is `vsa.yaml` in workspace root?
2. Is VSA CLI installed? (`vsa --version`)
3. Check Output panel for errors
4. Try restarting language server

**Fix:**
```bash
# Ensure CLI is installed
vsa --version

# Restart VS Code
Cmd+Q, reopen VS Code

# Check extension logs
View → Output → VSA Language Server
```

### Validation Not Running

**Check settings:**
```json
{
  "vsa.enabled": true,  // Must be true
  "vsa.validateOnSave": true
}
```

### No Auto-Completion

**For vsa.yaml:**
1. Ensure file is named exactly `vsa.yaml`
2. Check YAML extension is installed
3. Reload window

### Performance Issues

**If slow:**
```json
{
  "vsa.validateOnType": false,  // Disable type validation
  "vsa.validateOnSave": true    // Only validate on save
}
```

## Best Practices

### ✅ Do's

- Enable validate on save
- Use quick fixes for common issues
- Check Problems panel regularly
- Keep extension updated
- Use YAML auto-completion

### ❌ Don'ts

- Don't disable validation entirely
- Don't ignore boundary violations
- Don't commit with errors showing
- Don't work without extension
- Don't disable quick fixes

## Advanced Features

### Code Actions

Right-click in editor:
- "Generate matching event"
- "Create test file"
- "Add to integration events"

### Hover Information

Hover over files:
```typescript
PlaceOrderHandler
// VSA: Command handler for place-order feature
// Context: orders
// Required files: ✅ All present
```

### Go to Definition

Press `F12` on event name:
- Jumps to integration event definition
- Or domain event in same feature

## Comparison: CLI vs Extension

| Feature | CLI | Extension |
|---------|-----|-----------|
| Validation | ✅ On demand | ✅ Real-time |
| Quick Fixes | ❌ | ✅ |
| YAML Auto-complete | ❌ | ✅ |
| Inline Errors | ❌ | ✅ |
| CI/CD | ✅ | ❌ |
| Watch Mode | ✅ | ✅ Auto |

**Use both:** Extension for dev, CLI for CI/CD.

## Next Steps

- **Real-Time Validation** - Features built into the extension
- **Commands** - Use `Cmd+Shift+P` to see all VSA commands
- **YAML Schema** - Auto-completion is provided automatically

---

**Tip:** The extension makes VSA feel native to VS Code. Once installed, you'll wonder how you coded without it!

