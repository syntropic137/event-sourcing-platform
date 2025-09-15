# Rust Event Sourcing SDK Makefile

.PHONY: help build test check fmt clippy clean qa qa-fast qa-full setup

help:
	@echo "Rust Event Sourcing SDK - Available Targets:"
	@echo ""
	@echo "  build     - Build the library"
	@echo "  test      - Run tests"
	@echo "  check     - Check compilation"
	@echo "  fmt       - Format code"
	@echo "  clippy    - Run clippy lints"
	@echo "  qa        - Fast QA (fmt, clippy, check, unit tests)"
	@echo "  qa-full   - Full QA (fmt, clippy, check, full tests)"
	@echo "  clean     - Clean build artifacts"
	@echo "  setup     - Setup development environment"

setup:
	@echo "Setting up Rust SDK development environment..."
	@command -v cargo >/dev/null 2>&1 || { echo "Rust not found. Please install Rust."; exit 1; }
	@echo "✅ Rust SDK setup complete"

build:
	cargo build

test:
	cargo test

check:
	cargo check

fmt:
	cargo fmt --all -- --check

clippy:
	cargo clippy --all-targets --all-features -- -D warnings


qa-fast: fmt clippy check
	cargo test --lib
	cargo test --doc
	@echo "✅ Fast QA passed for Rust SDK"

qa: qa-fast
	@echo "⚠️  The slow tests have NOT run. For full QA, run: make qa-full"

qa-full: fmt clippy check test
	@echo "✅ Full QA checks passed for Rust SDK"

clean:
	cargo clean
