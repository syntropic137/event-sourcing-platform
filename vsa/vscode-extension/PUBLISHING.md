# Publishing the VSA VS Code Extension

## Prerequisites

1. Install `vsce` (VS Code Extension Manager):
```bash
npm install -g @vscode/vsce
```

2. Create a Visual Studio Marketplace publisher account:
   - Go to https://marketplace.visualstudio.com/manage
   - Sign in with Microsoft account
   - Create a publisher (e.g., `neuralempowerment`)

3. Create a Personal Access Token (PAT):
   - Go to https://dev.azure.com/[your-org]/_usersSettings/tokens
   - Create a new token with `Marketplace (Publish)` scope
   - Save the token securely

## Local Packaging

Create a `.vsix` file for local installation or testing:

```bash
cd vsa/vscode-extension
npm install
npm run compile
vsce package
```

This creates `vsa-vscode-0.1.0.vsix`.

### Install Locally

```bash
code --install-extension vsa-vscode-0.1.0.vsix
```

## Publishing to Marketplace

### First Time Setup

```bash
vsce login neuralempowerment
# Enter your PAT when prompted
```

### Publish

```bash
# Dry run (check what will be published)
vsce publish --dry-run

# Publish patch version (0.1.0 → 0.1.1)
vsce publish patch

# Publish minor version (0.1.0 → 0.2.0)
vsce publish minor

# Publish major version (0.1.0 → 1.0.0)
vsce publish major

# Or specify version explicitly
vsce publish 0.1.0
```

## Pre-publish Checklist

- [ ] All features tested manually
- [ ] README is comprehensive
- [ ] CHANGELOG is up to date
- [ ] Icon is present and looks good
- [ ] License file is included
- [ ] Repository URL is correct
- [ ] Publisher name is correct
- [ ] Version number follows semver
- [ ] No sensitive data in the package
- [ ] All dependencies are declared
- [ ] Extension works with latest VS Code version

## Publishing Workflow

1. **Update Version**
   ```bash
   npm version patch  # or minor/major
   ```

2. **Update CHANGELOG**
   - Add new version section
   - List all changes

3. **Commit Changes**
   ```bash
   git add .
   git commit -m "chore: bump version to 0.1.1"
   git tag v0.1.1
   git push origin main --tags
   ```

4. **Publish**
   ```bash
   vsce publish
   ```

5. **Verify**
   - Check marketplace listing: https://marketplace.visualstudio.com/items?itemName=neuralempowerment.vsa-vscode
   - Test installation: `code --install-extension neuralempowerment.vsa-vscode`

## Unpublishing

⚠️ **Warning:** Unpublishing can break existing users' installations.

```bash
vsce unpublish neuralempowerment.vsa-vscode
```

## Marketplace Details

### Display Name
Vertical Slice Architecture

### Short Description
VS Code extension for Vertical Slice Architecture validation and code generation

### Categories
- Linters
- Snippets
- Other

### Tags
- vertical-slice
- architecture
- event-sourcing
- ddd
- cqrs

### Links
- Repository: https://github.com/neuralempowerment/event-sourcing-platform
- Issues: https://github.com/neuralempowerment/event-sourcing-platform/issues
- Documentation: https://github.com/neuralempowerment/event-sourcing-platform/tree/main/vsa

## CI/CD Integration

### GitHub Actions

Create `.github/workflows/publish-extension.yml`:

```yaml
name: Publish Extension

on:
  push:
    tags:
      - 'v*'

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - uses: actions/setup-node@v3
        with:
          node-version: '18'
          
      - name: Install dependencies
        working-directory: vsa/vscode-extension
        run: npm ci
        
      - name: Compile
        working-directory: vsa/vscode-extension
        run: npm run compile
        
      - name: Publish to VS Marketplace
        working-directory: vsa/vscode-extension
        run: npx vsce publish -p ${{ secrets.VSCE_PAT }}
```

Add `VSCE_PAT` secret to repository settings.

## Troubleshooting

### "Error: Missing publisher name"
Add `"publisher": "neuralempowerment"` to package.json

### "Error: Icon not found"
Make sure `icon.svg` or `icon.png` exists in the extension root

### "Error: README.md not found"
Ensure README.md exists and is not in .vscodeignore

### "Error: Missing repository URL"
Add repository URL to package.json

### Extension not activating
Check activation events in package.json

