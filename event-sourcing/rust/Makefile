# Rust Event Sourcing SDK Makefile

.PHONY: help build test check fmt clippy clean qa qa-fast qa-full setup test-postgres test-xlang test-xlang-fixtures

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


# --tests includes the live-server integration tests; they run the real gRPC
# event store in-process on the memory backend (sub-second, no Docker).
# Set TEST_DATABASE_URL to run them against the Postgres backend.
qa-fast: fmt clippy check
	cargo test --tests
	cargo test --doc
	@echo "✅ Fast QA passed for Rust SDK"

# Requires TEST_DATABASE_URL (a Postgres database the tests may write to).
test-postgres:
	@test -n "$$TEST_DATABASE_URL" || { echo "TEST_DATABASE_URL must be set"; exit 1; }
	cargo test --features postgres --tests

# Live cross-language round trips (ADR-027) in all six directions between
# TypeScript, Python and Rust. Needs node (after `pnpm install` at the repo
# root) and uv.
# The TS peer loads the built SDK (dist), so build it first.
test-xlang:
	pnpm --dir ../.. --filter @eventstore/sdk-ts --filter @syntropic137/event-sourcing-typescript run build
	cargo test --test xlang_live -- --ignored

# Regenerate the golden fixtures from the real TypeScript, Python and Rust
# encoders (typescript-legacy.json is frozen). The envelope is a one-way door:
# review any diff in tests/fixtures/xlang.
test-xlang-fixtures:
	pnpm --dir ../.. --filter @eventstore/sdk-ts --filter @syntropic137/event-sourcing-typescript run build
	node tests/xlang/ts_peer.cjs capture tests/fixtures/xlang/typescript.json
	uv run --quiet --project ../python --extra grpc python tests/xlang/py_peer.py capture tests/fixtures/xlang/python.json
	XLANG_UPDATE_FIXTURES=1 cargo test --test xlang_golden rust_fixture_is_current

qa: qa-fast
	@echo "⚠️  The slow tests have NOT run. For full QA, run: make qa-full"

qa-full: fmt clippy check test
	@echo "✅ Full QA checks passed for Rust SDK"

clean:
	cargo clean
