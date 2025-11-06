# VSA Manager - Manual Testing Guide

This guide walks you through testing the entire VSA Manager system manually.

## Prerequisites

- Rust toolchain installed
- Node.js 18+ and npm/pnpm
- Docker and Docker Compose

## Part 1: Build the VSA CLI

```bash
cd /path/to/event-sourcing-platform_vsa_20251104/vsa

# Build the CLI
cargo build --release --bin vsa

# Verify build
./target/release/vsa --version

# Optional: Install globally
sudo cp target/release/vsa /usr/local/bin/
```

**Expected Output:**
```
vsa 0.1.0
```

## Part 2: Test VSA Validation

### Test Example 1: Todo List

```bash
cd examples/01-todo-list-ts

# Run validation
../../target/release/vsa validate
```

**Expected Output:**
```
🔍 Validating VSA structure...
📁 Root: src/contexts
🗣️  Language: typescript
✅ All checks passed!
```

### Test Example 2: Library Management

```bash
cd ../02-library-management-ts

# Run validation
../../target/release/vsa validate
```

**Expected Output:**
```
🔍 Validating VSA structure...
📁 Root: src/contexts
🗣️  Language: typescript

⚠️  6 Warning(s)
  ! Feature 'remove-book' in context 'catalog' is missing tests
  ! Feature 'add-book' in context 'catalog' is missing tests
  ...

✅ Validation passed with warnings
```

## Part 3: Test Feature Generation

### Generate a New Feature

```bash
# Still in 02-library-management-ts
../../target/release/vsa generate --context catalog --feature update-book
```

**Expected Output:**
```
🚧 Generating feature 'update-book' in context 'catalog'...

✅ Created feature files:
  ├─ src/contexts/catalog/update-book/UpdateBookCommand.ts
  ├─ src/contexts/catalog/update-book/BookUpdatedEvent.ts
  ├─ src/contexts/catalog/update-book/UpdateBookHandler.ts
  └─ src/contexts/catalog/update-book/UpdateBook.test.ts

💡 Next steps:
  1. Implement business logic in UpdateBookHandler
  2. Add tests in UpdateBook.test.ts
  3. Run: vsa validate
```

### Verify Generated Files

```bash
# Check the generated files
ls -la src/contexts/catalog/update-book/

# View a generated file
cat src/contexts/catalog/update-book/UpdateBookCommand.ts
```

**Expected:** Files should exist and contain boilerplate code.

### Clean Up Test Feature

```bash
rm -rf src/contexts/catalog/update-book
```

## Part 4: Test Feature Listing

```bash
# List all features
../../target/release/vsa list
```

**Expected Output:**
```
📦 Contexts
  ├─ catalog
    ├─ remove-book
    └─ add-book
  ├─ lending
    ├─ return-book
    ├─ mark-overdue
    └─ borrow-book
  └─ notifications
    ├─ send-notification
    └─ event-subscribers
```

## Part 5: Test Watch Mode

```bash
# Start watch mode (in a terminal)
../../target/release/vsa validate --watch
```

**Expected Output:**
```
🔍 Validating VSA structure...
📁 Root: src/contexts
🗣️  Language: typescript
✅ All checks passed!

👀 Watching for changes... (Press Ctrl+C to exit)
```

### Trigger a Change

In another terminal:
```bash
# Make a change to trigger re-validation
touch src/contexts/catalog/add-book/AddBookCommand.ts
```

**Expected:** Watch mode should detect the change and re-run validation.

Press `Ctrl+C` to exit watch mode.

## Part 6: Test E2E Infrastructure

### Start the Infrastructure

```bash
cd ../  # Go to vsa/examples/

# Start event-store + PostgreSQL
make start-infra
```

**Expected Output:**
```
🚀 Starting event-store infrastructure...
[+] Running 2/2
 ✔ Container eventstore-postgres  Healthy
 ✔ Container event-store           Healthy
⏳ Waiting for event-store to be ready...
✅ Event store is ready!
✅ Infrastructure started successfully

Event Store gRPC: localhost:50051
PostgreSQL:       localhost:5432
```

### Verify Services are Running

```bash
# Check status
make status

# Or use Docker directly
docker ps
```

**Expected:** Should see `event-store` and `eventstore-postgres` containers running.

## Part 7: Test E2E Tests

### Test Todo List Example

```bash
cd 01-todo-list-ts

# Install dependencies (if not already done)
npm install

# Run E2E tests
npm run test:e2e
```

**Expected Output:**
```
PASS  tests/e2e/todoFlow.e2e.test.ts
  Todo List E2E Tests
    Complete Todo Workflow
      ✓ should create, complete, and delete a task (523ms)
      ✓ should handle multiple tasks (412ms)
      ✓ should persist events across application restarts (287ms)
    Error Handling
      ✓ should handle non-existent task gracefully (156ms)
      ✓ should handle optimistic concurrency (298ms)

Test Suites: 1 passed, 1 total
Tests:       5 passed, 5 total
```

### Test Library Management Example

```bash
cd ../02-library-management-ts

# Install dependencies
npm install

# Run E2E tests
npm run test:e2e
```

**Expected Output:**
```
PASS  tests/e2e/library.e2e.test.ts
  Library Management E2E Tests
    Catalog Context
      ✓ should add a book to the catalog (389ms)
      ✓ should remove a book from the catalog (512ms)
      ✓ should prevent adding duplicate books (298ms)
    Lending Context
      ✓ should borrow and return a book (445ms)
      ✓ should mark a loan as overdue (367ms)
    Cross-Context Integration
      ✓ should send notifications when borrowing a book (601ms)
      ✓ should handle the complete library workflow (723ms)
    Error Handling
      ✓ should handle invalid commands gracefully (145ms)
      ✓ should prevent borrowing with invalid due date (178ms)
      ✓ should prevent double returns (289ms)

Test Suites: 1 passed, 1 total
Tests:       10 passed, 10 total
```

### Run All Tests at Once

```bash
cd ..  # Back to examples/

# Run all E2E tests
make test-all
```

**Expected:** Both test suites should pass.

## Part 8: Stop Infrastructure

```bash
# From vsa/examples/
make stop-infra
```

**Expected Output:**
```
🛑 Stopping infrastructure...
[+] Running 2/2
 ✔ Container event-store           Removed
 ✔ Container eventstore-postgres   Removed
✅ Infrastructure stopped
```

## Part 9: Test VS Code Extension (Optional)

If you have VS Code installed:

```bash
cd ../vscode-extension

# Install dependencies
npm install

# Compile
npm run compile

# Package
npm run package
```

**Expected:** Creates `vsa-vscode-0.1.0.vsix` file.

### Install in VS Code

```bash
code --install-extension vsa-vscode-0.1.0.vsix
```

### Test in VS Code

1. Open an example project in VS Code
2. Open `vsa.yaml` - should have auto-completion
3. Make an error (e.g., delete a required file)
4. Save - should see diagnostics
5. Use Command Palette (`Cmd+Shift+P`) - should see VSA commands

## Part 10: Test Init Command

```bash
# Create a test project
cd /tmp
mkdir test-vsa-project
cd test-vsa-project

# Initialize
/path/to/vsa/target/release/vsa init --language typescript

# Verify files were created
ls -la
cat vsa.yaml
```

**Expected Output:**
```
✅ Created VSA configuration: vsa.yaml

Next steps:
  1. Review and customize vsa.yaml
  2. Create your first context: mkdir -p src/contexts/your-context
  3. Generate a feature: vsa generate -c your-context -f your-feature
  4. Validate structure: vsa validate
```

**Clean up:**
```bash
cd /tmp
rm -rf test-vsa-project
```

## Part 11: Test Interactive Generation

```bash
cd /path/to/vsa/examples/02-library-management-ts

# Generate with interactive prompts
../../target/release/vsa generate --context catalog --feature add-review
```

Follow the prompts:
- Add fields (name, type, required)
- Choose to include aggregate
- See the files created

**Clean up:**
```bash
rm -rf src/contexts/catalog/add-review
```

## Troubleshooting

### Event Store Not Running

**Symptom:** E2E tests fail with connection errors
**Fix:**
```bash
cd vsa/examples
make start-infra
```

### Permission Denied (vsa command)

**Fix:**
```bash
chmod +x /path/to/vsa/target/release/vsa
```

### Dependencies Not Installed

**Fix:**
```bash
cd vsa/examples/01-todo-list-ts
npm install
```

## Success Criteria

✅ VSA CLI builds successfully
✅ Validation works on both examples
✅ Feature generation creates correct files
✅ List command shows all features
✅ Watch mode detects changes
✅ Infrastructure starts successfully
✅ E2E tests pass for Todo List (5/5)
✅ E2E tests pass for Library Management (10/10)
✅ All tests can be run with `make test-all`
✅ Infrastructure stops cleanly

## Summary

If all tests pass, you have successfully verified:
- ✅ VSA CLI core functionality
- ✅ Code generation
- ✅ Validation engine
- ✅ Watch mode
- ✅ E2E testing infrastructure
- ✅ Integration with event-sourcing platform
- ✅ Full stack examples

**The VSA Manager is working perfectly!** 🎉

