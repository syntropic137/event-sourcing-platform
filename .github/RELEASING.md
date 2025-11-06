# Release Guide

This guide explains how to create a new release and publish the `@event-sourcing-platform/typescript` package to GitHub Packages.

## Prerequisites

- Write access to the repository
- All tests passing on `main` branch
- CHANGELOG.md updated

## Release Process

### 1. Update Version

Update the version in `event-sourcing/typescript/package.json`:

```bash
cd event-sourcing/typescript
npm version [major|minor|patch]
```

This will:
- Update the version in package.json
- Create a git commit
- Create a git tag

### 2. Update CHANGELOG

Add release notes to `event-sourcing/typescript/CHANGELOG.md`:

```markdown
## [X.Y.Z] - YYYY-MM-DD

### Added
- New features

### Changed
- Breaking changes
- Updates

### Fixed
- Bug fixes
```

### 3. Push Changes

```bash
git push origin main
git push origin --tags
```

### 4. Create GitHub Release

1. Go to https://github.com/NeuralEmpowerment/event-sourcing-platform/releases
2. Click "Draft a new release"
3. Select the tag you just created (e.g., `v0.1.0`)
4. Title: `v0.1.0` (match the tag)
5. Description: Copy the CHANGELOG entry
6. Click "Publish release"

This will automatically trigger the `publish.yml` workflow to:
- Run all tests
- Build the package
- Publish to GitHub Packages
- Verify the publication

### 5. Verify Publication

Check the Actions tab to ensure the publish workflow succeeded:
https://github.com/NeuralEmpowerment/event-sourcing-platform/actions

Once complete, the package will be available at:
https://github.com/NeuralEmpowerment/event-sourcing-platform/packages

## Semantic Versioning

We follow [Semantic Versioning](https://semver.org/):

- **Major (X.0.0)**: Breaking changes
- **Minor (0.X.0)**: New features (backward compatible)
- **Patch (0.0.X)**: Bug fixes (backward compatible)

### Examples

- `feat: add new decorator` → Minor bump (0.1.0 → 0.2.0)
- `fix: resolve command dispatch bug` → Patch bump (0.1.0 → 0.1.1)
- `feat!: change aggregate API` → Major bump (0.1.0 → 1.0.0)

## Conventional Commits

Use conventional commit messages to help determine version bumps:

```
feat: add new feature (minor)
fix: fix bug (patch)
feat!: breaking change (major)
docs: update docs (no version change)
chore: maintenance (no version change)
refactor: code refactoring (patch)
perf: performance improvement (patch)
test: add tests (no version change)
```

## Pre-release Versions

For beta releases:

```bash
npm version prerelease --preid=beta
# Creates: 0.1.0-beta.0
```

For alpha releases:

```bash
npm version prerelease --preid=alpha
# Creates: 0.1.0-alpha.0
```

## Troubleshooting

### Publish Failed

1. Check Actions logs: https://github.com/NeuralEmpowerment/event-sourcing-platform/actions
2. Common issues:
   - Version already exists
   - Tests failed
   - Build errors

### Version Mismatch

If the package.json version doesn't match the tag:

```bash
# Update package.json manually
cd event-sourcing/typescript
# Edit package.json to match the tag version

# Commit the fix
git add package.json
git commit -m "fix: correct version number"
git push origin main

# Delete and recreate the tag
git tag -d v0.1.0
git push origin :refs/tags/v0.1.0
git tag v0.1.0
git push origin v0.1.0
```

### Manual Publish

If automatic publishing fails, you can publish manually:

```bash
cd event-sourcing/typescript
pnpm run build
echo "//npm.pkg.github.com/:_authToken=$GITHUB_TOKEN" > .npmrc
pnpm publish --no-git-checks
```

## Testing a Release

Before creating a real release, test with a dry-run:

1. Go to Actions → Publish to GitHub Packages
2. Click "Run workflow"
3. Check "Dry run"
4. Click "Run workflow"

This will run all tests and build the package without publishing.

## Post-Release

After a successful release:

1. ✅ Verify package is accessible:
   ```bash
   npm view @event-sourcing-platform/typescript@X.Y.Z
   ```

2. ✅ Update documentation if needed

3. ✅ Announce the release (if major/minor)

4. ✅ Close related GitHub issues

## Support

For questions or issues, open a GitHub issue or contact the team.

