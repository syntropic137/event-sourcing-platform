.PHONY: help setup test test-unit test-integration qa qa-fast lint format type-check clean build gen-proto

help:
	@echo "Python Event Sourcing SDK - Available commands:"
	@echo ""
	@echo "Setup:"
	@echo "  setup         - Install dependencies with uv"
	@echo "  clean         - Remove build artifacts"
	@echo ""
	@echo "Testing:"
	@echo "  test          - Run all tests"
	@echo "  test-unit     - Run unit tests only"
	@echo "  test-integration - Run integration tests"
	@echo ""
	@echo "Quality:"
	@echo "  qa            - Run full QA (lint, type-check, test)"
	@echo "  qa-fast       - Quick QA (lint, type-check, unit tests)"
	@echo "  lint          - Run ruff linter"
	@echo "  format        - Format code with black"
	@echo "  type-check    - Run mypy type checker"
	@echo ""
	@echo "Build:"
	@echo "  build         - Build distribution package"
	@echo "  gen-proto     - Generate gRPC stubs from proto files"

setup:
	uv sync --all-extras

test:
	uv run pytest

test-unit:
	uv run pytest tests/unit/

test-integration:
	uv run pytest -m integration

qa: setup lint type-check test
	@echo "✅ Full QA passed"

qa-fast: setup lint type-check test-unit
	@echo "✅ Fast QA passed"

lint:
	uv run ruff check src tests

format: setup
	uv run black src tests
	uv run ruff check --fix src tests

type-check:
	uv run mypy src

clean:
	rm -rf build/ dist/ *.egg-info
	rm -rf .pytest_cache .mypy_cache .ruff_cache
	rm -rf htmlcov/ .coverage
	find . -type d -name __pycache__ -exec rm -rf {} +
	find . -type f -name "*.pyc" -delete

build: clean
	uv build

gen-proto:
	@echo "Generating gRPC stubs..."
	@mkdir -p src/event_sourcing/proto/eventstore/v1
	@touch src/event_sourcing/proto/__init__.py
	@touch src/event_sourcing/proto/eventstore/__init__.py
	@touch src/event_sourcing/proto/eventstore/v1/__init__.py
	@uv run python -m grpc_tools.protoc \
		-I ../../event-store/eventstore-proto/proto \
		--python_out=src/event_sourcing/proto \
		--grpc_python_out=src/event_sourcing/proto \
		--pyi_out=src/event_sourcing/proto \
		../../event-store/eventstore-proto/proto/eventstore/v1/eventstore.proto
	@echo "✅ Proto stubs generated successfully"

