# Root Makefile for Event Sourcing Platform
# Coordinates builds across event-store, event-sourcing, examples, and tools

.PHONY: help build clean test qa qa-fast qa-full setup dev-setup
.PHONY: event-store event-sourcing examples tools
.PHONY: start-services stop-services smoke-test run-event-store

# Default target
help:
	@echo "Event Sourcing Platform - Available Targets:"
	@echo ""
	@echo "Core Components:"
	@echo "  event-store       - Build and test the Rust event store"
	@echo "  event-sourcing    - Build and test event sourcing SDKs"
	@echo "  examples          - Build and test all examples"
	@echo "  tools             - Build development tools"
	@echo ""
	@echo "Development:"
	@echo "  setup             - Initial project setup"
	@echo "  dev-setup         - Setup development environment"
	@echo "  build             - Build all components"
	@echo "  test              - Run all tests"
	@echo "  qa                - Run fast QA checks (no slow tests/coverage)"
	@echo "  qa-full           - Run full QA sweep (may be slow)"
	@echo "  clean             - Clean all build artifacts"
	@echo ""
	@echo "Services:"
	@echo "  start-services    - Start development services (PostgreSQL, etc.)"
	@echo "  stop-services     - Stop development services"
	@echo "  run-event-store   - Run the Rust gRPC event store server"
	@echo ""
	@echo "Examples:"
	@echo "  examples-001      - Basic event store usage"
	@echo "  examples-002      - Simple aggregate"
	@echo "  examples-003      - Multiple aggregates"
	@echo "  examples-004      - CQRS patterns"
	@echo "  examples-005      - Projections"
	@echo "  examples-006      - Event bus"
	@echo "  examples-007      - E-commerce complete"
	@echo "  examples-008      - Banking complete"
	@echo "  examples-009      - Inventory complete"

# Setup targets
setup:
	@echo "Setting up Event Sourcing Platform..."
	@echo "Checking prerequisites..."
	@command -v cargo >/dev/null 2>&1 || { echo "Rust/Cargo not found. Please install Rust."; exit 1; }
	@command -v node >/dev/null 2>&1 || { echo "Node.js not found. Please install Node.js."; exit 1; }
	@command -v python3 >/dev/null 2>&1 || { echo "Python 3 not found. Please install Python 3."; exit 1; }
	@command -v docker >/dev/null 2>&1 || { echo "Docker not found. Please install Docker."; exit 1; }
	@echo "✅ Prerequisites check passed"
	$(MAKE) dev-setup

dev-setup:
	@echo "Setting up development environment..."
	@if [ -d event-store ]; then \
		echo "Setting up event-store..."; \
		cd event-store && $(MAKE) setup || true; \
	fi
	@if [ -d event-sourcing ]; then \
		echo "Setting up event-sourcing SDKs..."; \
		cd event-sourcing && $(MAKE) setup || true; \
	fi
	@echo "✅ Development environment setup complete"

# Build targets
build: event-store event-sourcing examples tools

event-store:
	@echo "Building event-store..."
	@if [ -d event-store ]; then \
		cd event-store && $(MAKE) build; \
	else \
		echo "⚠️  event-store directory not found"; \
	fi

event-sourcing:
	@echo "Building event-sourcing SDKs..."
	@if [ -d event-sourcing ]; then \
		cd event-sourcing && $(MAKE) build; \
	else \
		echo "⚠️  event-sourcing directory not found"; \
	fi

examples:
	@echo "Building examples..."
	@if [ -d examples ]; then \
		cd examples && $(MAKE) build; \
	else \
		echo "⚠️  examples directory not found"; \
	fi

tools:
	@echo "Building tools..."
	@if [ -d tools ]; then \
		cd tools && $(MAKE) build; \
	else \
		echo "⚠️  tools directory not found"; \
	fi

# Test targets
test: test-event-store test-event-sourcing test-examples

test-event-store:
	@echo "Testing event-store..."
	@if [ -d event-store ]; then \
		cd event-store && $(MAKE) test; \
	fi

test-event-sourcing:
	@echo "Testing event-sourcing..."
	@if [ -d event-sourcing ]; then \
		cd event-sourcing && $(MAKE) test; \
	fi

test-examples:
	@echo "Testing examples..."
	@if [ -d examples ]; then \
		cd examples && $(MAKE) test; \
	fi

# QA targets
qa-fast: qa-event-store-fast qa-event-sourcing-fast
	@echo "✅ Fast QA passed across modules"

qa: qa-fast
	@echo "⚠️  The slow tests and coverage have NOT run. For full QA, run: make qa-full"

qa-full: qa-event-store-full qa-event-sourcing-full
	@echo "✅ Full QA passed across modules"

qa-event-store:
	@echo "QA checks for event-store..."
	@if [ -d event-store ]; then \
		cd event-store && $(MAKE) qa; \
	fi

qa-event-store-fast:
	@echo "Fast QA for event-store..."
	@if [ -d event-store ]; then \
		cd event-store && $(MAKE) qa-fast; \
	fi

qa-event-store-full:
	@echo "Full QA for event-store..."
	@if [ -d event-store ]; then \
		cd event-store && $(MAKE) qa-full; \
	fi

qa-event-sourcing:
	@echo "QA checks for event-sourcing..."
	@if [ -d event-sourcing ]; then \
		cd event-sourcing && $(MAKE) qa; \
	fi

qa-event-sourcing-fast:
	@echo "Fast QA for event-sourcing..."
	@if [ -d event-sourcing ]; then \
		cd event-sourcing && $(MAKE) qa-fast; \
	fi

qa-event-sourcing-full:
	@echo "Full QA for event-sourcing..."
	@if [ -d event-sourcing ]; then \
		cd event-sourcing && $(MAKE) qa-full; \
	fi

qa-examples:
	@echo "QA checks for examples..."
	@if [ -d examples ]; then \
		cd examples && $(MAKE) qa; \
	fi

# Service management
start-services:
	@echo "Starting development services..."
	@if [ -f docker-compose.dev.yml ]; then \
		docker-compose -f docker-compose.dev.yml up -d; \
	else \
		echo "Creating basic PostgreSQL service..."; \
		docker run -d --name event-store-postgres \
			-e POSTGRES_PASSWORD=postgres \
			-e POSTGRES_DB=eventstore \
			-p 5432:5432 \
			postgres:15; \
	fi
	@echo "✅ Services started"

stop-services:
	@echo "Stopping development services..."
	@if [ -f docker-compose.dev.yml ]; then \
		docker-compose -f docker-compose.dev.yml down; \
	else \
		docker stop event-store-postgres || true; \
		docker rm event-store-postgres || true; \
	fi
	@echo "✅ Services stopped"

# Smoke test
smoke-test:
	@echo "Running smoke tests..."
	@if [ -d event-store ]; then \
		cd event-store && $(MAKE) smoke || echo "Event store smoke test failed"; \
	fi
	@echo "✅ Smoke tests complete"

# Run the Rust event store server by delegating to the sub-makefile
run-event-store:
	@if [ -d event-store ]; then \
		echo "Starting event-store server (see event-store/Makefile for options)..."; \
		cd event-store && $(MAKE) run; \
	else \
		echo "⚠️  event-store directory not found"; \
		exit 2; \
	fi

# Clean targets
clean:
	@echo "Cleaning all build artifacts..."
	@if [ -d event-store ]; then cd event-store && $(MAKE) clean; fi
	@if [ -d event-sourcing ]; then cd event-sourcing && $(MAKE) clean; fi
	@if [ -d examples ]; then cd examples && $(MAKE) clean; fi
	@if [ -d tools ]; then cd tools && $(MAKE) clean; fi
	@echo "✅ Clean complete"

# Example-specific targets
examples-001:
	@if [ -d examples/001-basic-store ]; then \
		cd examples/001-basic-store && $(MAKE) run; \
	elif [ -d examples/001-basic-store-ts ]; then \
		echo "Running 001-basic-store TypeScript example..."; \
		echo "Bootstrapping workspace and building TypeScript SDK..."; \
		pnpm -w install; \
		$(MAKE) -C event-store/sdks/sdk-ts build || true; \
		$(MAKE) -C event-sourcing/typescript build || true; \
		cd examples/001-basic-store-ts && \
		( TS_NODE_TRANSPILE_ONLY=1 pnpm run dev || ( pnpm run build && pnpm run start ) || echo "Install deps with: pnpm install" ); \
	else \
		echo "Example 001 not found"; \
	fi

examples-002:
	@if [ -d examples/002-simple-aggregate ]; then \
		cd examples/002-simple-aggregate && $(MAKE) run; \
	elif [ -d examples/002-simple-aggregate-ts ]; then \
		echo "Running 002-simple-aggregate TypeScript example..."; \
		echo "Bootstrapping workspace and building TypeScript SDK..."; \
		pnpm -w install; \
		$(MAKE) -C event-store/sdks/sdk-ts build || true; \
		$(MAKE) -C event-sourcing/typescript build || true; \
		cd examples/002-simple-aggregate-ts && \
		( TS_NODE_TRANSPILE_ONLY=1 pnpm run dev || ( pnpm run build && pnpm run start ) || echo "Install deps with: pnpm install" ); \
	else \
		echo "Example 002 not found"; \
	fi

examples-003:
	@if [ -d examples/003-multiple-aggregates ]; then \
		cd examples/003-multiple-aggregates && $(MAKE) run; \
	else \
		echo "Example 003 not found"; \
	fi

examples-004:
	@if [ -d examples/004-cqrs-patterns ]; then \
		cd examples/004-cqrs-patterns && $(MAKE) run; \
	else \
		echo "Example 004 not found"; \
	fi

examples-005:
	@if [ -d examples/005-projections ]; then \
		cd examples/005-projections && $(MAKE) run; \
	else \
		echo "Example 005 not found"; \
	fi

examples-006:
	@if [ -d examples/006-event-bus ]; then \
		cd examples/006-event-bus && $(MAKE) run; \
	else \
		echo "Example 006 not found"; \
	fi

examples-007:
	@if [ -d examples/007-ecommerce-complete ]; then \
		cd examples/007-ecommerce-complete && $(MAKE) run; \
	else \
		echo "Example 007 not found"; \
	fi

examples-008:
	@if [ -d examples/008-banking-complete ]; then \
		cd examples/008-banking-complete && $(MAKE) run; \
	else \
		echo "Example 008 not found"; \
	fi

examples-009:
	@if [ -d examples/009-inventory-complete ]; then \
		cd examples/009-inventory-complete && $(MAKE) run; \
	else \
		echo "Example 009 not found"; \
	fi
