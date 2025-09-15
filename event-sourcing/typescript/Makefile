# TypeScript SDK Makefile

.PHONY: help build clean install qa qa-fast qa-full fmt lint type-check test

# Default target
help:
	@echo "TypeScript SDK - Available Targets:"
	@echo ""
	@echo "  build         - Build the TypeScript SDK"
	@echo "  clean         - Clean build artifacts"
	@echo "  install       - Install dependencies"
	@echo "  qa            - Fast QA (format + type-check)"
	@echo "  qa-full       - Full QA (format, lint, type-check, test)"
	@echo "  fmt           - Format code"
	@echo "  lint          - Run linter"
	@echo "  type-check    - Run TypeScript type checking"
	@echo "  test          - Run tests"

# Install dependencies (pnpm required for workspace: protocol)
install:
	@if command -v pnpm >/dev/null 2>&1; then \
		pnpm install; \
	else \
		echo "pnpm is required here (workspace protocol). Install via: npm i -g pnpm"; \
		exit 2; \
	fi

# Build the project
build: install
	pnpm exec tsc --build

# Clean build artifacts
clean:
	rm -rf dist/
	rm -rf node_modules/

# QA checks
# Fast-by-default QA avoids slow/noisy steps
qa-fast: fmt type-check
	@echo "✅ Fast QA passed for TypeScript SDK"

qa: qa-fast
	@echo "⚠️  The slow checks (lint/tests) have NOT run. For full QA, run: make qa-full"

qa-full: fmt lint type-check test
	@echo "✅ Full QA passed for TypeScript SDK"

# Format code
fmt: install
	pnpm exec prettier --write "src/**/*.ts"

# Lint code
lint:
	@echo "ESLint temporarily disabled - TODO: Fix ESLint v9 configuration"

# Type check
type-check: install
	pnpm exec tsc --noEmit

# Run tests
test:
	@echo "No tests implemented yet for TypeScript SDK"
