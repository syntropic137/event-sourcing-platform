"""
In-memory implementation of ProjectionStore.

This is for TEST ENVIRONMENTS ONLY (per ADR-004).
"""

import os
from typing import Any

from event_sourcing.core.checkpoint import ProjectionReadStore, ProjectionStore


def _assert_test_environment() -> None:
    """Ensure we are running in a test environment.

    Per ADR-004, mock/in-memory implementations must validate they are
    running in the test environment. This prevents accidental use in
    production where data would be lost on restart.

    Raises:
        RuntimeError: If not in a test environment
    """
    is_test = (
        os.getenv("PYTEST_CURRENT_TEST") is not None
        or os.getenv("TEST_ENV") == "true"
        or os.getenv("IS_TEST") == "true"
        or os.getenv("NODE_ENV") == "test"
        or os.getenv("ENVIRONMENT") == "test"
    )

    if not is_test:
        raise RuntimeError(
            "MemoryProjectionStore is for test environments only. "
            "Set TEST_ENV=true or use a persistent ProjectionStore for production. "
            "See ADR-004 for details."
        )


class MemoryProjectionStore:
    """In-memory projection store for unit tests.

    Stores projection read-model data in nested dictionaries.
    Data is lost when the process exits.

    Usage (in tests only):
        store = MemoryProjectionStore()
        await store.save("repo_cost", "org/repo", {"total": 42.0})
        data = await store.get("repo_cost", "org/repo")
    """

    def __init__(self) -> None:
        _assert_test_environment()
        self._data: dict[str, dict[str, dict[str, Any]]] = {}

    async def save(self, projection: str, key: str, data: dict[str, Any]) -> None:
        """Save or update a projection record."""
        if projection not in self._data:
            self._data[projection] = {}
        self._data[projection][key] = data

    async def get(self, projection: str, key: str) -> dict[str, Any] | None:
        """Get a single projection record by key."""
        return self._data.get(projection, {}).get(key)

    async def get_all(self, projection: str) -> list[dict[str, Any]]:
        """Get all records for a projection."""
        return list(self._data.get(projection, {}).values())

    async def delete(self, projection: str, key: str) -> None:
        """Delete a projection record."""
        if projection in self._data:
            self._data[projection].pop(key, None)

    async def delete_all(self, projection: str) -> None:
        """Delete all records for a projection."""
        self._data.pop(projection, None)

    async def query(
        self,
        projection: str,
        filters: dict[str, Any] | None = None,
        order_by: str | None = None,
        limit: int | None = None,
        offset: int = 0,
    ) -> list[dict[str, Any]]:
        """Query projection records with optional filtering."""
        records = list(self._data.get(projection, {}).values())

        if filters:
            records = [
                r for r in records if all(r.get(k) == v for k, v in filters.items())
            ]

        if order_by:
            reverse = order_by.startswith("-")
            field = order_by.lstrip("-")
            records.sort(key=lambda r: r.get(field, ""), reverse=reverse)

        records = records[offset:]
        if limit is not None:
            records = records[:limit]

        return records

    async def get_by_prefix(
        self, projection: str, prefix: str
    ) -> list[tuple[str, dict[str, Any]]]:
        """Get all records whose key starts with the given prefix."""
        results: list[tuple[str, dict[str, Any]]] = []
        for key, data in self._data.get(projection, {}).items():
            if key.startswith(prefix):
                results.append((key, data))
        return results

    def clear(self) -> None:
        """Clear all data (test utility)."""
        self._data.clear()


# Protocol compliance assertions (static check)
_store: type[ProjectionStore] = MemoryProjectionStore
_read_store: type[ProjectionReadStore] = MemoryProjectionStore
