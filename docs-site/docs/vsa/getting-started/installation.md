---
sidebar_position: 1
---

# Installation

Get the VSA Manager tools installed and ready to use.

## Prerequisites

Before installing VSA Manager, ensure you have:

- **Rust** (1.70+) - For building the CLI tool
- **Node.js** (18+) - For TypeScript projects
- **VS Code** (optional) - For IDE integration

### Install Rust

If you don't have Rust installed:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

Verify installation:

```bash
rustc --version
cargo --version
```

## Install VSA CLI

### Option 1: Build from Source (Recommended)

Clone the repository and build the CLI:

```bash
# Clone the repository
git clone https://github.com/neuralempowerment/event-sourcing-platform.git
cd event-sourcing-platform/vsa/vsa-cli

# Build release binary
cargo build --release

# Install to system path
sudo cp target/release/vsa /usr/local/bin/

# Verify installation
vsa --version
```

### Option 2: Add to PATH (Development)

For development or if you don't want to install globally:

```bash
# Build the CLI
cd vsa/vsa-cli
cargo build --release

# Add to your shell profile (~/.bashrc, ~/.zshrc, etc.)
export PATH="$PATH:/path/to/event-sourcing-platform/vsa/target/release"

# Reload your shell
source ~/.zshrc  # or ~/.bashrc
```

### Verify Installation

```bash
vsa --version
# Output: vsa 0.1.0

vsa --help
# Shows available commands
```

## Install VS Code Extension

The VS Code extension provides real-time validation, quick fixes, and auto-completion.

### Installation Steps

```bash
# Navigate to extension directory
cd vsa/vscode-extension

# Install dependencies
npm install

# Package the extension
npm run package

# Install the extension
code --install-extension vsa-vscode-0.1.0.vsix
```

### Verify Extension

1. Open VS Code
2. Press `Cmd+Shift+P` (Mac) or `Ctrl+Shift+P` (Windows/Linux)
3. Type "VSA" - you should see VSA commands available
4. Open the Output panel and select "VSA Language Server" to see logs

### Extension Features

Once installed, the extension provides:

- ✅ **Real-time Validation** - See errors as you type
- ✅ **Quick Fixes** - Auto-fix common issues
- ✅ **Command Palette** - Generate features, validate architecture
- ✅ **YAML Schema** - Auto-completion for `vsa.yaml`
- ✅ **Inline Diagnostics** - Violations shown in Problems panel

## Project Setup

After installation, you can set up your first project:

### Initialize a New Project

```bash
# Create project directory
mkdir my-vsa-project
cd my-vsa-project

# Initialize with TypeScript
vsa init --language typescript

# Or Python
vsa init --language python
```

This creates:

```
my-vsa-project/
├── vsa.yaml              # Configuration file
├── src/
│   └── contexts/         # Root for vertical slices
└── package.json          # (TypeScript only)
```

### Configure for Existing Project

Add VSA to an existing project:

```bash
cd your-existing-project

# Initialize VSA
vsa init --language typescript --root src/contexts

# Edit vsa.yaml to match your structure
```

## Configuration File

The `vsa.yaml` file is the heart of your VSA configuration:

```yaml
version: 1
language: typescript
root: src/contexts

# Optional: Framework integration
framework:
  name: event-sourcing-platform
  aggregate_class: AggregateRoot
  aggregate_import: "@event-sourcing-platform/typescript"

# Define bounded contexts
bounded_contexts:
  - name: orders
    description: Order management and processing

# Validation rules
validation:
  require_tests: true
  require_handler: true
  require_aggregate: false

# Integration events configuration
integration_events:
  path: ../_shared/integration-events
  events: {}
```

## Verify Setup

Test that everything is working:

```bash
# Validate (should pass with no features yet)
vsa validate

# List features (should be empty)
vsa list

# Generate a test feature
vsa generate orders test-feature

# Validate again
vsa validate
```

## Troubleshooting

### Command Not Found

If `vsa` command is not found:

```bash
# Check if binary exists
ls -l /usr/local/bin/vsa

# Check PATH
echo $PATH

# Try absolute path
/usr/local/bin/vsa --version
```

### Permission Denied

If you get permission errors:

```bash
# Make binary executable
chmod +x /usr/local/bin/vsa

# Or use sudo for installation
sudo cp target/release/vsa /usr/local/bin/
```

### VS Code Extension Not Loading

1. Check VS Code version (requires 1.80+)
2. Reload VS Code window: `Cmd+Shift+P` → "Reload Window"
3. Check Output panel: "VSA Language Server" for errors
4. Ensure `vsa` CLI is in PATH

### Build Errors

If Rust build fails:

```bash
# Update Rust
rustup update

# Clean and rebuild
cargo clean
cargo build --release
```

## Next Steps

Now that you have VSA Manager installed:

1. **[Quick Start](./quick-start)** - Create your first VSA project
2. **[Your First Feature](./your-first-feature)** - Generate and implement a feature
3. **[CLI Commands](../cli/commands)** - Learn all available commands

## Updating

To update to the latest version:

```bash
# Pull latest changes
cd event-sourcing-platform
git pull origin main

# Rebuild CLI
cd vsa/vsa-cli
cargo build --release
sudo cp target/release/vsa /usr/local/bin/

# Rebuild VS Code extension
cd ../vscode-extension
npm install
npm run package
code --install-extension vsa-vscode-0.1.0.vsix
```

## Uninstalling

To remove VSA Manager:

```bash
# Remove CLI
sudo rm /usr/local/bin/vsa

# Uninstall VS Code extension
code --uninstall-extension vsa-vscode
```

---

**Ready to build?** Continue to the [Quick Start Guide](./quick-start) →

