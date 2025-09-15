# Event Sourcing SDKs Makefile
# Coordinates builds across Rust, TypeScript, and Python SDKs

.PHONY: help build clean test qa qa-fast qa-full setup
.PHONY: rust typescript python
.PHONY: test-rust test-typescript test-python
.PHONY: qa-rust qa-typescript qa-python

help:
	@echo "Event Sourcing SDKs - Available Targets:"
	@echo ""
	@echo "Build:"
	@echo "  build         - Build all SDKs"
	@echo "  rust          - Build Rust SDK"
	@echo "  typescript    - Build TypeScript SDK"
	@echo "  python        - Build Python SDK"
	@echo ""
	@echo "Test:"
	@echo "  test          - Test all SDKs"
	@echo "  test-rust     - Test Rust SDK"
	@echo "  test-typescript - Test TypeScript SDK"
	@echo "  test-python   - Test Python SDK"
	@echo ""
	@echo "Quality:"
	@echo "  qa            - Fast QA (no slow tests/coverage)"
	@echo "  qa-full       - Full QA (may be slow)"
	@echo "  qa-rust       - Run QA on Rust SDK"
	@echo "  qa-typescript - Run QA on TypeScript SDK"
	@echo "  qa-python     - Run QA on Python SDK"
	@echo ""
	@echo "Setup:"
	@echo "  setup         - Setup all SDKs"
	@echo "  clean         - Clean all build artifacts"

setup:
	@echo "Setting up Event Sourcing SDKs..."
	@if [ -d rust ]; then cd rust && $(MAKE) setup; fi
	@if [ -d typescript ]; then cd typescript && $(MAKE) setup; fi
	@if [ -d python ]; then cd python && $(MAKE) setup; fi

build: rust typescript python

rust:
	@echo "Building Rust SDK..."
	@if [ -d rust ]; then \
		cd rust && $(MAKE) build; \
	else \
		echo "⚠️  Rust SDK not found"; \
	fi

typescript:
	@echo "Building TypeScript SDK..."
	@if [ -d typescript ]; then \
		cd typescript && $(MAKE) build; \
	else \
		echo "⚠️  TypeScript SDK not found"; \
	fi

python:
	@echo "Building Python SDK..."
	@if [ -d python ]; then \
		cd python && $(MAKE) build; \
	else \
		echo "⚠️  Python SDK not found"; \
	fi

test: test-rust test-typescript test-python

test-rust:
	@echo "Testing Rust SDK..."
	@if [ -d rust ]; then cd rust && $(MAKE) test; fi

test-typescript:
	@echo "Testing TypeScript SDK..."
	@if [ -d typescript ]; then cd typescript && $(MAKE) test; fi

test-python:
	@echo "Testing Python SDK..."
	@if [ -d python ]; then cd python && $(MAKE) test; fi

qa-fast: qa-rust-fast qa-typescript
	@echo "✅ Fast QA passed for Event Sourcing SDKs"

qa: qa-fast
	@echo "⚠️  The slow tests and coverage have NOT run. For full QA, run: make qa-full"

qa-full: qa-rust-full qa-typescript qa-python
	@echo "✅ Full QA passed for Event Sourcing SDKs"

qa-rust:
	@echo "QA checks for Rust SDK..."
	@if [ -d rust ]; then cd rust && $(MAKE) qa; fi

qa-rust-fast:
	@echo "Fast QA for Rust SDK..."
	@if [ -d rust ]; then cd rust && $(MAKE) qa-fast; fi

qa-rust-full:
	@echo "Full QA for Rust SDK..."
	@if [ -d rust ]; then cd rust && $(MAKE) qa-full; fi

qa-typescript:
	@echo "QA checks for TypeScript SDK..."
	@if [ -d typescript ]; then cd typescript && $(MAKE) qa; fi

qa-python:
	@echo "QA checks for Python SDK..."
	@if [ -d python ] && [ -f python/Makefile ]; then cd python && $(MAKE) qa; else echo "(skipping: python SDK not configured)"; fi

clean:
	@echo "Cleaning Event Sourcing SDKs..."
	@if [ -d rust ]; then cd rust && $(MAKE) clean; fi
	@if [ -d typescript ]; then cd typescript && $(MAKE) clean; fi
