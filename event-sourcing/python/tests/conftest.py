"""Pytest configuration and fixtures."""

import pytest


@pytest.fixture
def sample_aggregate_id() -> str:
    """Sample aggregate ID for tests."""
    return "test-aggregate-123"


@pytest.fixture
def sample_event_data() -> dict[str, str]:
    """Sample event data for tests."""
    return {"order_id": "order-123", "customer_id": "customer-456"}
