---
sidebar_position: 4
---

# Watch Mode

Real-time validation for continuous feedback during development.

## Overview

Watch mode continuously monitors your codebase for changes and automatically re-validates when files are modified. This provides instant feedback as you code.

```bash
$ vsa validate --watch

👀 Watching: src/contexts
   Press Ctrl+C to stop

✅ Initial validation passed (3 features, 0 errors)

[12:34:56] File changed: src/contexts/orders/place-order/PlaceOrderHandler.ts
[12:34:56] ✅ Validating...
[12:34:56] ✅ Validation passed (3 features, 0 errors)
```

## Starting Watch Mode

### Basic Usage

```bash
vsa validate --watch
```

### With Custom Config

```bash
vsa --config vsa.dev.yaml validate --watch
```

### With Verbose Logging

```bash
vsa --verbose validate --watch
```

## Features

### Instant Feedback

Validation runs immediately after file changes:

```bash
[12:34:56] File changed: PlaceOrderHandler.ts
[12:34:56] ✅ Validating... (took 42ms)
[12:34:56] ✅ All checks passed
```

### Smart Detection

Only re-validates affected contexts:

```bash
[12:35:12] File changed: contexts/orders/place-order/PlaceOrderHandler.ts
[12:35:12] 🔍 Validating context: orders
[12:35:12] ✅ orders validated (1 feature)
```

### Error Highlighting

Errors are immediately visible:

```bash
[12:36:45] File changed: contexts/orders/cancel-order/CancelOrderCommand.ts
[12:36:45] ✅ Validating...
[12:36:45] ❌ Validation failed

❌ Missing handler
   Feature: cancel-order
   Expected: CancelOrderHandler.ts
```

## Watched Files

Watch mode monitors:

- All TypeScript/Python/Rust files in contexts
- Configuration file (`vsa.yaml`)
- Integration event files
- Subscriber handlers

### File Patterns

```
src/contexts/
  **/*.ts
  **/*.py
  **/*.rs
  
vsa.yaml

_shared/integration-events/
  **/*.ts
  **/*.py
  **/*.rs
```

## Watch Mode Output

### Initial Validation

```bash
$ vsa validate --watch

🚀 VSA Watch Mode

📂 Watching: src/contexts
📄 Config: vsa.yaml

✅ Initial validation passed
   3 contexts
   12 features  
   0 errors
   
👀 Watching for changes... (Press Ctrl+C to stop)
```

### File Change Detected

```bash
[12:34:56] File changed: orders/place-order/PlaceOrderHandler.ts
[12:34:56] 🔍 Validating context: orders
[12:34:57] ✅ Validation passed (took 45ms)
```

### Multiple Changes

```bash
[12:35:23] Files changed (3):
           - orders/place-order/PlaceOrderHandler.ts
           - orders/place-order/PlaceOrder.test.ts
           - orders/place-order/PlaceOrderCommand.ts
[12:35:23] 🔍 Validating context: orders
[12:35:23] ✅ Validation passed (took 52ms)
```

### Configuration Change

```bash
[12:36:10] Config changed: vsa.yaml
[12:36:10] 🔄 Reloading configuration...
[12:36:10] ✅ Configuration reloaded
[12:36:10] 🔍 Re-validating all contexts...
[12:36:11] ✅ Validation passed (took 123ms)
```

## Integration with Development Tools

### With VS Code

Run watch mode in integrated terminal:

```bash
# Terminal 1: VSA Watch
vsa validate --watch

# Terminal 2: Development server
npm run dev
```

### With Test Watch

Run tests and validation together:

```bash
# Terminal 1: VSA Watch
vsa validate --watch

# Terminal 2: Test Watch
npm test -- --watch

# Terminal 3: Development
# Your coding here
```

### With Hot Reload

Perfect for development servers:

```bash
# Terminal 1: VSA Watch
vsa validate --watch

# Terminal 2: Next.js/Vite
npm run dev

# Both provide instant feedback!
```

## Performance

### Fast Incremental Validation

Watch mode only re-validates what changed:

```
Single file change:
  ✅ orders context only (42ms)

Multiple files in same context:
  ✅ orders context only (52ms)

Config file change:
  🔄 All contexts (123ms)
```

### Debouncing

Multiple rapid changes are debounced:

```bash
[12:34:56.123] File changed: PlaceOrderHandler.ts
[12:34:56.245] File changed: PlaceOrder.test.ts
[12:34:56.367] File changed: PlaceOrderCommand.ts
[12:34:56.500] 🔍 Validating... (debounced 3 changes)
[12:34:56.545] ✅ Validation passed
```

## Error Handling

### Syntax Errors

Watch mode continues even with syntax errors:

```bash
[12:37:22] File changed: PlaceOrderHandler.ts
[12:37:22] ❌ Syntax error in file
           Cannot parse TypeScript file
           
👀 Still watching... (fix the error and save)

[12:37:45] File changed: PlaceOrderHandler.ts
[12:37:45] ✅ Validation passed
```

### Missing Files

```bash
[12:38:10] File deleted: CancelOrderHandler.ts
[12:38:10] ❌ Validation failed

❌ Missing handler
   Feature: cancel-order
   Expected: CancelOrderHandler.ts

👀 Still watching...
```

## Common Workflows

### Daily Development

```bash
# Morning: Start watch mode
vsa validate --watch

# Keep it running all day
# Instant feedback on every save

# Evening: Stop with Ctrl+C
```

### Feature Development

```bash
# 1. Start watch mode
vsa validate --watch

# 2. Generate feature
vsa generate -c orders -f place-order

# 3. Watch validates automatically
[12:40:12] Feature created
[12:40:12] ✅ Validating...
[12:40:12] ✅ All checks passed

# 4. Develop with instant feedback
# Save file → Auto-validate → Fix errors → Repeat
```

### Refactoring

```bash
# 1. Start watch mode
vsa validate --watch

# 2. Make changes
# Rename files, move features, etc.

# 3. Watch catches violations immediately
[12:42:33] ❌ Boundary violation detected
           (Fix immediately)

# 4. Validation passes when done
[12:43:15] ✅ Refactoring looks good!
```

## Troubleshooting

### Watch Not Detecting Changes

**Problem:** Files change but watch doesn't trigger

**Solutions:**
```bash
# 1. Check file is in watched directory
vsa list  # Verify feature is detected

# 2. Try saving again
# Some editors delay writes

# 3. Restart watch mode
Ctrl+C
vsa validate --watch

# 4. Check verbose output
vsa --verbose validate --watch
```

### Too Many Validation Runs

**Problem:** Validation runs constantly

**Cause:** Editor auto-save or build tools

**Solutions:**
```bash
# 1. Exclude build output
validation:
  exclude:
    - "**/dist/**"
    - "**/build/**"
    - "**/.next/**"

# 2. Adjust editor auto-save
# VS Code: Increase auto-save delay
```

### Slow Validation

**Problem:** Watch mode is slow

**Solutions:**
```bash
# 1. Exclude unnecessary paths
validation:
  exclude:
    - "**/node_modules/**"
    - "**/.git/**"

# 2. Reduce context size
# Split large contexts into smaller ones

# 3. Use faster storage
# SSD vs HDD makes big difference
```

## Best Practices

### ✅ Do's

- Run watch mode during active development
- Keep watch terminal visible for feedback
- Fix errors as they appear
- Use with test watch mode
- Restart after config changes

### ❌ Don'ts

- Don't ignore watch errors
- Don't run multiple watch instances
- Don't commit with watch errors
- Don't watch production builds
- Don't disable for "just this once"

## Advanced Usage

### Custom Debounce (Future)

```yaml
# vsa.yaml
watch:
  debounce_ms: 500  # Default: 300ms
```

### Selective Context Watch (Future)

```bash
# Watch specific context
vsa validate --watch --context orders

# Watch multiple contexts
vsa validate --watch --context orders,inventory
```

### Watch with Auto-Fix (Future)

```bash
# Auto-fix issues as they're detected
vsa validate --watch --fix
```

## CI/CD Note

**Don't use watch mode in CI/CD!**

```bash
# ❌ In CI (will hang)
vsa validate --watch

# ✅ In CI (runs once and exits)
vsa validate
```

## Keyboard Shortcuts

- **Ctrl+C** - Stop watch mode
- **Ctrl+Z** - Pause (resume with `fg`)

## Exit Codes

Watch mode never exits with error (keeps running).

To stop:
- Press Ctrl+C
- Send SIGTERM
- Close terminal

## Tips

### Tip 1: Split Screen

```
┌─────────────────┬─────────────────┐
│                 │                 │
│  Editor         │  Terminal       │
│  (Code)         │  (VSA Watch)    │
│                 │                 │
│                 │  ✅ Instant     │
│                 │     Feedback    │
└─────────────────┴─────────────────┘
```

### Tip 2: Notification on Error

```bash
# macOS
vsa validate --watch || osascript -e 'display notification "VSA validation failed"'

# Linux
vsa validate --watch || notify-send "VSA validation failed"
```

### Tip 3: Log to File

```bash
vsa validate --watch 2>&1 | tee vsa-watch.log
```

## Next Steps

- **[Commands](./commands)** - All CLI commands
- **[Validation Rules](./validation-rules)** - What gets validated
- **[VS Code Extension](../ide/vscode-extension)** - IDE integration

---

**Pro tip:** Leave watch mode running all day for continuous feedback. It's like having a pair programmer watching for mistakes!

