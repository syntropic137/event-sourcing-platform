.PHONY: help build test check fmt clippy clean install run-example

help: ## Show this help message
	@echo 'Usage: make [target]'
	@echo ''
	@echo 'Available targets:'
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2}'

build: ## Build all crates
	cargo build --all-features

test: ## Run all tests
	cargo test --all-features --workspace

check: ## Run all checks (fmt, clippy, test)
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings
	cargo test --all-features --workspace

fmt: ## Format code
	cargo fmt --all

clippy: ## Run clippy
	cargo clippy --all-targets --all-features -- -D warnings

clean: ## Clean build artifacts
	cargo clean

install: ## Install vsa CLI locally
	cargo install --path vsa-cli

run-example: ## Run init example
	cargo run --bin vsa -- init --help

# Development helpers
watch: ## Watch and run tests on file changes
	cargo watch -x test

dev: ## Start development mode with auto-reload
	cargo watch -x 'run --bin vsa'

# Release
release: ## Build release binaries
	cargo build --release --all-features

# Documentation
docs: ## Build and open documentation
	cargo doc --all-features --no-deps --open

# VSA commands
vsa-init: ## Initialize VSA configuration
	cargo run --bin vsa -- init

vsa-validate: ## Validate VSA structure
	cargo run --bin vsa -- validate

vsa-list: ## List contexts and features
	cargo run --bin vsa -- list

