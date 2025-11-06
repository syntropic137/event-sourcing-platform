# Contributing to Event Sourcing Platform

Thank you for your interest in contributing! This guide will help you get started.

## Code of Conduct

Be respectful, inclusive, and professional in all interactions.

## Getting Started

### Prerequisites

- Node.js 18+ and pnpm
- Git
- A GitHub account

### Setup Development Environment

1. Fork the repository
2. Clone your fork:
   ```bash
   git clone https://github.com/YOUR_USERNAME/event-sourcing-platform.git
   cd event-sourcing-platform
   ```

3. Install dependencies:
   ```bash
   pnpm install
   ```

4. Build all packages:
   ```bash
   pnpm run build
   ```

5. Run tests:
   ```bash
   pnpm run test
   ```

## Development Workflow

### 1. Create a Branch

```bash
git checkout -b feat/your-feature-name
# or
git checkout -b fix/your-bug-fix
```

Branch naming:
- `feat/` - New features
- `fix/` - Bug fixes
- `docs/` - Documentation changes
- `refactor/` - Code refactoring
- `test/` - Test additions/changes
- `chore/` - Maintenance tasks

### 2. Make Changes

- Write code following our style guide
- Add tests for new functionality
- Update documentation as needed
- Follow the @CommandHandler pattern for event sourcing

### 3. Test Your Changes

```bash
# Run tests
cd event-sourcing/typescript
pnpm run test

# Run linter
pnpm run lint

# Fix linting issues
pnpm run lint:fix

# Build to ensure no errors
pnpm run build
```

### 4. Commit Your Changes

We use [Conventional Commits](https://www.conventionalcommits.org/):

```bash
git add .
git commit -m "feat: add new command handler feature"
```

Commit message format:
```
<type>(<scope>): <subject>

<body>

<footer>
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation
- `style`: Code style (formatting)
- `refactor`: Code refactoring
- `test`: Adding tests
- `chore`: Maintenance

Examples:
```bash
git commit -m "feat: add @CommandHandler decorator support"
git commit -m "fix: resolve aggregate initialization bug"
git commit -m "docs: update README with installation instructions"
git commit -m "feat!: change repository API (breaking change)"
```

### 5. Push and Create PR

```bash
git push origin feat/your-feature-name
```

Then:
1. Go to GitHub and create a Pull Request
2. Fill in the PR template
3. Link related issues
4. Wait for CI checks to pass
5. Request review from maintainers

## Pull Request Guidelines

### PR Title

Use conventional commit format:
```
feat: add new feature
fix: resolve bug
docs: update documentation
```

### PR Description

Include:
- What changed and why
- Related issue numbers (#123)
- Breaking changes (if any)
- Screenshots (for UI changes)
- Testing instructions

### PR Checklist

- [ ] Tests pass locally
- [ ] Linter passes
- [ ] Documentation updated
- [ ] CHANGELOG.md updated (for significant changes)
- [ ] Conventional commit messages used
- [ ] No merge conflicts

## Coding Standards

### TypeScript Style

- Use TypeScript strict mode
- Prefer `interface` over `type` for object shapes
- Use explicit return types for public methods
- Document public APIs with JSDoc comments

Example:
```typescript
/**
 * Handle a command by dispatching to the appropriate @CommandHandler method
 * @param command - The command to handle
 * @throws {Error} If no handler is found for the command type
 */
protected handleCommand<TCommand extends object>(command: TCommand): void {
  // implementation
}
```

### Event Sourcing Patterns

Follow the @CommandHandler pattern:

```typescript
// ✅ Good - Command as class
export class PlaceOrderCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly customerId: string,
    public readonly items: Item[]
  ) {}
}

// ✅ Good - Handler on aggregate
@Aggregate('Order')
export class OrderAggregate extends AggregateRoot<OrderEvent> {
  @CommandHandler('PlaceOrderCommand')
  placeOrder(command: PlaceOrderCommand): void {
    // validation
    this.initialize(command.aggregateId);
    this.apply(new OrderPlaced(...));
  }

  @EventSourcingHandler('OrderPlaced')
  private onOrderPlaced(event: OrderPlaced): void {
    // state updates only
  }
}

// ❌ Bad - Separate handler class
export class PlaceOrderHandler {
  handle(command: PlaceOrderCommand) { /* ... */ }
}
```

### Testing

- Write unit tests for all new features
- Use descriptive test names
- Follow AAA pattern (Arrange, Act, Assert)

Example:
```typescript
describe('OrderAggregate', () => {
  it('should place order when valid items provided', () => {
    // Arrange
    const aggregate = new OrderAggregate();
    const command = new PlaceOrderCommand('order-1', 'customer-1', [
      { productId: 'p1', quantity: 1, price: 10 }
    ]);

    // Act
    (aggregate as any).handleCommand(command);

    // Assert
    const events = aggregate.getUncommittedEvents();
    expect(events).toHaveLength(1);
    expect(events[0].eventType).toBe('OrderPlaced');
  });
});
```

## Project Structure

```
event-sourcing-platform/
├── event-sourcing/
│   └── typescript/           # Main library package
│       ├── src/
│       │   ├── core/        # Core abstractions
│       │   ├── client/      # Event store clients
│       │   └── utils/       # Utilities
│       └── tests/           # Unit tests
├── examples/                # Example applications
│   ├── 001-basic-store-ts/
│   └── ...
├── vsa/                     # VSA integration
│   └── examples/
└── docs-site/              # Documentation

```

## Common Tasks

### Adding a New Example

1. Create directory: `examples/XXX-example-name-ts/`
2. Copy structure from existing example
3. Update examples/README.md
4. Follow @CommandHandler pattern
5. Add README.md in example directory

### Updating Documentation

1. Edit files in `docs-site/docs/`
2. Test locally:
   ```bash
   cd docs-site
   npm run start
   ```
3. Verify no broken links:
   ```bash
   npm run build
   ```

### Adding a New Decorator

1. Add to `src/utils/decorators.ts`
2. Export from `src/index.ts`
3. Add tests in `tests/`
4. Update documentation
5. Add example usage

## CI/CD

All PRs automatically run:
- ✅ Tests (Node 18.x, 20.x)
- ✅ Linting
- ✅ Build verification
- ✅ Security scanning
- ✅ Documentation build

PRs cannot be merged until all checks pass.

## Release Process

See [RELEASING.md](.github/RELEASING.md) for the complete release process.

Only maintainers can create releases.

## Getting Help

- 📖 Read the [documentation](../README.md)
- 💬 Ask questions in GitHub Discussions
- 🐛 Report bugs via GitHub Issues
- 📧 Contact maintainers

## Recognition

Contributors will be recognized in:
- CHANGELOG.md
- GitHub contributors page
- Release notes

Thank you for contributing! 🎉

