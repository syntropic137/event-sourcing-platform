# TypeScript SDK Makefile

.PHONY: help build clean install qa qa-fast qa-full fmt lint type-check test

# Default target
help:
	@echo "TypeScript SDK - Available Targets:"
	@echo ""
	@echo "  install       - Install workspace dependencies"
	@echo "  build         - Build the SDK (tsc --build)"
	@echo "  clean         - Clean build artifacts"
	@echo "  test          - Run Jest test suite"
	@echo "  fmt           - Check formatting with Prettier"
	@echo "  lint          - Run ESLint"
	@echo "  type-check    - TypeScript --noEmit"
	@echo "  qa-fast       - fmt + lint + type-check"
	@echo "  qa            - qa-fast + tests"
	@echo "  qa-full       - qa + build"

# Install dependencies (pnpm required for workspace: protocol)
install:
	@pnpm install --filter @event-sourcing-platform/typescript...

# Build the project
build: install
	@pnpm --filter @eventstore/sdk-ts... run build
	@pnpm exec tsc --build

# Clean build artifacts
clean:
	rm -rf dist/
	rm -f tsconfig.tsbuildinfo

# QA checks
# Fast-by-default QA avoids slow/noisy steps
qa-fast: fmt lint type-check
	@echo "✅ Fast QA passed for TypeScript SDK"

qa: qa-fast test
	@echo "✅ QA (with tests) passed for TypeScript SDK"

qa-full: qa build
	@echo "✅ Full QA passed for TypeScript SDK"

# Format code
fmt:
	@pnpm --filter @event-sourcing-platform/typescript exec prettier --check "src/**/*.ts" "tests/**/*.ts"

# Lint code
lint:
	@pnpm --filter @event-sourcing-platform/typescript exec eslint "src" "tests" --ext .ts

# Type check (src and tests separately to catch all issues)
type-check:
	@echo "Type checking src/..."
	@pnpm --filter @event-sourcing-platform/typescript exec tsc --noEmit
	@echo "Type checking tests/..."
	@pnpm --filter @event-sourcing-platform/typescript exec tsc --noEmit --project tsconfig.jest.json

# Run tests
test: install
	@pnpm exec jest --runInBand
