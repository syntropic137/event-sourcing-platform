# Installing VSA Visualizer

## From the VSA Monorepo

If you're working within the VSA monorepo:

```bash
# From vsa/ directory
make visualizer-install
make visualizer-build

# Run visualizer
cd ../examples/002-simple-aggregate-ts
../../vsa/target/release/vsa manifest --include-domain | \
  node ../../vsa/vsa-visualizer/dist/index.js --output ./docs/architecture
```

## In Another Repository

### Option 1: Copy the Visualizer (Recommended for now)

```bash
# From your project root
cp -r path/to/vsa/vsa-visualizer ./tools/vsa-visualizer
cd tools/vsa-visualizer
npm install
npm run build

# Use it
vsa manifest --include-domain | node tools/vsa-visualizer/dist/index.js
```

### Option 2: Link via npm/pnpm (Development)

```bash
# In the vsa-visualizer directory
cd vsa/vsa-visualizer
npm link

# In your project
npm link @vsa/visualizer

# Use it
vsa manifest --include-domain | vsa-visualizer
```

### Option 3: Install via Git (Future)

Once published, this will be available:

```bash
npm install -g @vsa/visualizer
# or
pnpm add -g @vsa/visualizer

# Then use anywhere
vsa manifest --include-domain | vsa-visualizer
```

## Using with VSA CLI

### Prerequisites

1. VSA CLI installed with domain manifest support
2. Node.js 18+ installed
3. A VSA project with aggregates, commands, and events

### Generate Documentation

```bash
# Basic usage
vsa manifest --include-domain | vsa-visualizer

# With custom output
vsa manifest --include-domain | vsa-visualizer --output ./architecture-docs

# Save manifest first (if you want to version it)
vsa manifest --include-domain > architecture-manifest.json
vsa-visualizer architecture-manifest.json --output ./docs/arch
```

## Verification

Test that it's working:

```bash
# Check version
vsa-visualizer --version

# Check help
vsa-visualizer --help

# Test with example (from VSA monorepo)
cd examples/002-simple-aggregate-ts
vsa manifest --include-domain | vsa-visualizer --verbose
```

You should see output like:

```
✅ Documentation generated successfully!

📊 Summary:
   Aggregates: 1
   Commands:   2
   Events:     2

📝 Generated files:
   - docs/architecture/OVERVIEW.md
   - docs/architecture/aggregates/OrderAggregate.md
   - docs/architecture/aggregates/README.md
```

## Troubleshooting

### "vsa: command not found"

Install VSA CLI first:
```bash
cd vsa/vsa-cli
cargo install --path .
```

### "Manifest does not contain domain data"

Add the `--include-domain` flag:
```bash
vsa manifest --include-domain
```

### "node: command not found"

Install Node.js 18+ from https://nodejs.org/

### TypeScript build errors

Make sure you're using Node 18+:
```bash
node --version  # Should be v18 or higher
npm install     # Reinstall dependencies
npm run build   # Rebuild
```

## CI/CD Integration

### GitHub Actions

```yaml
name: Generate Architecture Docs

on:
  push:
    branches: [main]

jobs:
  docs:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Setup Node.js
        uses: actions/setup-node@v3
        with:
          node-version: '18'
      
      - name: Install VSA Visualizer
        run: |
          cd tools/vsa-visualizer
          npm install
          npm run build
      
      - name: Generate Docs
        run: |
          vsa manifest --include-domain | \
            node tools/vsa-visualizer/dist/index.js --output ./docs/architecture
      
      - name: Commit docs
        run: |
          git config user.name "GitHub Actions"
          git config user.email "actions@github.com"
          git add docs/architecture
          git commit -m "docs: update architecture diagrams" || true
          git push || true
```

## Next Steps

Once documentation is generated:

1. **Commit the docs** - Architecture diagrams are version controlled
2. **View on GitHub** - Mermaid diagrams render automatically
3. **Share with team** - Link to `OVERVIEW.md` for onboarding
4. **Keep updated** - Regenerate when domain changes

For more details, see [README.md](./README.md)
