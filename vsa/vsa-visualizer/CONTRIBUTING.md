# Contributing to VSA Visualizer

Thank you for your interest in contributing to VSA Visualizer! This document provides guidelines and information for contributors.

## 🎯 Development Philosophy

VSA Visualizer follows these principles:

1. **Maintainability**: Code should be easy to understand and modify
2. **Test Coverage**: All features must have comprehensive tests
3. **Backward Compatibility**: Schema version 1.0.0 must remain supported
4. **Type Safety**: Leverage TypeScript's type system fully
5. **Documentation**: Code should be self-documenting with clear naming

## 🚀 Getting Started

### Prerequisites

- Node.js 18+
- npm or pnpm
- VSA CLI (for integration testing)
- Git

### Setup Development Environment

```bash
# Clone and navigate
cd vsa/vsa-visualizer

# Install dependencies
npm install

# Build
npm run build

# Run tests
npm test

# Run tests in watch mode
npm test -- --watch
```

## 📝 Development Workflow

### 1. Create a Branch

```bash
git checkout -b feature/your-feature-name
# or
git checkout -b fix/bug-description
```

### 2. Make Changes

Follow the existing code structure:

```
src/
├── generators/          # Add new generators here
│   ├── base-generator.ts
│   └── your-generator.ts
├── types/              # Extend manifest types here
├── manifest/           # Parser modifications
└── utils/              # Utility functions
```

### 3. Write Tests

Every feature needs tests:

```typescript
// tests/generators/your-generator.test.ts
import { YourGenerator } from '../../src/generators/your-generator';

describe('YourGenerator', () => {
  it('should generate expected output', () => {
    // Test implementation
  });
});
```

### 4. Run QA Checks

```bash
# Run all checks
npm run check-fix

# Individual checks
npm run type-check
npm run lint
npm run format
npm test
```

### 5. Commit Changes

Use conventional commits:

```bash
git add .
git commit -m "feat(generator): add new diagram type

- Implement XYZ generator
- Add tests for XYZ
- Update documentation"
```

### Commit Message Format

```
<type>(<scope>): <subject>

<body>

<footer>
```

**Types:**
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation only
- `refactor`: Code restructuring
- `test`: Adding tests
- `chore`: Maintenance

**Scopes:**
- `generator`: Generator changes
- `cli`: CLI changes
- `parser`: Manifest parser
- `types`: Type definitions
- `test`: Test infrastructure

## 🧪 Testing Guidelines

### Test Coverage Requirements

- **Minimum**: 80% coverage
- **Unit tests**: Test individual functions/classes
- **Integration tests**: Test end-to-end CLI flows
- **Fixtures**: Add test data to `tests/fixtures/`

### Writing Good Tests

```typescript
describe('Feature', () => {
  // Arrange
  const manifest = createTestManifest();
  const generator = new Generator(manifest);
  
  // Act
  const output = generator.generate();
  
  // Assert
  expect(output).toContain('expected content');
});
```

### Test Organization

```
tests/
├── fixtures/              # Test data
│   └── test-manifest.json
├── generators/            # Generator tests
│   ├── overview-generator.test.ts
│   └── aggregate-generator.test.ts
├── integration/           # End-to-end tests
│   └── cli.test.ts
└── manifest/              # Parser tests
    └── parser.test.ts
```

## 🎨 Code Style

### TypeScript Guidelines

```typescript
// ✅ Good: Clear, typed, documented
/**
 * Generate a summary section
 * @param aggregates List of aggregates to summarize
 * @returns Markdown formatted summary
 */
private generateSummary(aggregates: Aggregate[]): string {
  const count = aggregates.length;
  return this.section(`Summary: ${count} aggregates`, 2);
}

// ❌ Bad: Unclear, any types, no docs
private gen(data: any): any {
  return data.map(x => x.name).join(',');
}
```

### Naming Conventions

- **Classes**: PascalCase (`OverviewGenerator`)
- **Functions/Methods**: camelCase (`generateSummary`)
- **Constants**: UPPER_SNAKE_CASE (`MANIFEST_SCHEMA_VERSION`)
- **Files**: kebab-case (`overview-generator.ts`)

### File Organization

```typescript
// 1. Imports (external, then internal)
import { Command } from 'commander';
import { Manifest } from '../types/manifest';

// 2. Constants
const DEFAULT_OUTPUT = 'docs/architecture';

// 3. Interfaces/Types
interface Options {
  output: string;
  verbose: boolean;
}

// 4. Main class/function
export class Generator {
  // ...
}

// 5. Helper functions (private)
function sanitizeId(name: string): string {
  // ...
}
```

## 🔧 Adding New Features

### Adding a New Generator

1. **Create generator class**:

```typescript
// src/generators/new-generator.ts
import { BaseGenerator } from './base-generator';

export class NewGenerator extends BaseGenerator {
  protected getTitle(): string {
    return 'New Feature';
  }

  generate(): string {
    // Implementation
  }
}
```

2. **Add tests**:

```typescript
// tests/generators/new-generator.test.ts
describe('NewGenerator', () => {
  // Tests
});
```

3. **Integrate into CLI**:

```typescript
// src/index.ts
import { NewGenerator } from './generators/new-generator';

// In action handler:
const newGen = new NewGenerator(manifest);
const output = newGen.generate();
```

4. **Update documentation**:
   - README.md
   - Type definitions
   - Examples

### Extending Manifest Schema

If you need new manifest fields:

1. **Update types**:

```typescript
// src/types/manifest.ts
export interface DomainManifest {
  // ... existing fields
  newField?: NewFieldType[];  // Add here
}
```

2. **Update parser validation**:

```typescript
// src/manifest/parser.ts
function validateDomain(domain: unknown): void {
  // Add validation for newField
}
```

3. **Add tests**:

```typescript
// tests/manifest/parser.test.ts
it('should parse newField when present', () => {
  // Test
});
```

4. **Update documentation**: Note schema changes

## 📚 Documentation

### Code Comments

```typescript
/**
 * JSDoc for public APIs
 * @param manifest The VSA manifest to process
 * @returns Generated markdown content
 */
export function generateDocs(manifest: Manifest): string {
  // Implementation comments for complex logic
  return content;
}
```

### README Updates

When adding features, update:
- Features list
- Usage examples
- CLI options table
- Troubleshooting (if applicable)

## 🐛 Reporting Bugs

### Before Reporting

1. Check existing issues
2. Verify you're using latest version
3. Try with `--verbose` flag
4. Test with minimal example

### Bug Report Template

```markdown
**Description**
Clear description of the bug

**To Reproduce**
1. Generate manifest: `vsa manifest --include-domain`
2. Run visualizer: `vsa-visualizer manifest.json`
3. See error

**Expected Behavior**
What you expected to happen

**Actual Behavior**
What actually happened

**Environment**
- VSA Visualizer version: 0.1.0
- VSA CLI version: 0.6.1-beta
- Node.js version: 18.0.0
- OS: macOS / Linux / Windows

**Manifest Sample** (if relevant)
```json
{
  "version": "0.6.1-beta",
  ...
}
```
```

## 🎯 Feature Requests

We welcome feature requests! Please:

1. Check if it already exists in issues/roadmap
2. Describe the use case clearly
3. Provide examples if possible
4. Consider backward compatibility

## 🔍 Code Review Process

All contributions go through code review:

1. **Automated checks**: Tests, lint, type-check must pass
2. **Manual review**: Code quality, design, documentation
3. **Testing**: Reviewer tests the changes
4. **Approval**: At least one approval required

### Review Checklist

- [ ] Tests pass and coverage maintained
- [ ] Code follows style guidelines
- [ ] Documentation updated
- [ ] Backward compatible (or breaking change noted)
- [ ] Commit messages follow convention
- [ ] No unnecessary dependencies added

## 🚢 Release Process

Releases are handled by maintainers:

1. Version bump in `package.json`
2. Update CHANGELOG.md
3. Tag release
4. Publish to npm (if applicable)
5. Update documentation

## 💬 Questions?

- Open a discussion on GitHub
- Check existing documentation
- Ask in pull request comments

## 📄 License

By contributing, you agree that your contributions will be licensed under the MIT License.

---

Thank you for contributing to VSA Visualizer! 🎉
