"""Tests for ProjectionStore and ProjectionReadStore protocols."""

from typing import Any

import pytest

from event_sourcing.core.checkpoint import ProjectionReadStore, ProjectionStore

# ============================================================================
# Inline test implementation (mirrors the pattern in test_checkpoint.py)
# ============================================================================


class InMemoryStore:
    """Minimal in-memory implementation for testing protocol compliance."""

    def __init__(self) -> None:
        self._data: dict[str, dict[str, dict[str, Any]]] = {}

    async def save(self, projection: str, key: str, data: dict[str, Any]) -> None:
        if projection not in self._data:
            self._data[projection] = {}
        self._data[projection][key] = data

    async def get(self, projection: str, key: str) -> dict[str, Any] | None:
        return self._data.get(projection, {}).get(key)

    async def get_all(self, projection: str) -> list[dict[str, Any]]:
        return list(self._data.get(projection, {}).values())

    async def delete(self, projection: str, key: str) -> None:
        if projection in self._data:
            self._data[projection].pop(key, None)

    async def delete_all(self, projection: str) -> None:
        self._data.pop(projection, None)

    async def query(
        self,
        projection: str,
        filters: dict[str, Any] | None = None,
        order_by: str | None = None,
        limit: int | None = None,
        offset: int = 0,
    ) -> list[dict[str, Any]]:
        records = list(self._data.get(projection, {}).values())
        if filters:
            records = [r for r in records if all(r.get(k) == v for k, v in filters.items())]
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
        return [
            (k, v)
            for k, v in self._data.get(projection, {}).items()
            if k.startswith(prefix)
        ]


# ============================================================================
# Protocol compliance tests
# ============================================================================


class TestProjectionStoreProtocol:
    """Tests for ProjectionStore protocol."""

    def test_protocol_runtime_checkable(self) -> None:
        """ProjectionStore should be runtime checkable."""
        store = InMemoryStore()
        assert isinstance(store, ProjectionStore)

    def test_projection_store_satisfies_read_store(self) -> None:
        """Any ProjectionStore should also satisfy ProjectionReadStore."""
        store = InMemoryStore()
        assert isinstance(store, ProjectionStore)
        assert isinstance(store, ProjectionReadStore)

    @pytest.mark.asyncio
    async def test_save_and_get(self) -> None:
        """Should save and retrieve a record."""
        store = InMemoryStore()
        data = {"total_cost": 42.5, "execution_count": 3}

        await store.save("repo_cost", "org/repo", data)
        result = await store.get("repo_cost", "org/repo")

        assert result == data

    @pytest.mark.asyncio
    async def test_get_nonexistent(self) -> None:
        """Should return None for nonexistent record."""
        store = InMemoryStore()

        result = await store.get("repo_cost", "nonexistent")

        assert result is None

    @pytest.mark.asyncio
    async def test_get_all(self) -> None:
        """Should return all records for a projection."""
        store = InMemoryStore()
        await store.save("repo_cost", "org/a", {"cost": 10})
        await store.save("repo_cost", "org/b", {"cost": 20})

        results = await store.get_all("repo_cost")

        assert len(results) == 2
        assert {"cost": 10} in results
        assert {"cost": 20} in results

    @pytest.mark.asyncio
    async def test_get_all_empty(self) -> None:
        """Should return empty list for nonexistent projection."""
        store = InMemoryStore()

        results = await store.get_all("nonexistent")

        assert results == []

    @pytest.mark.asyncio
    async def test_delete(self) -> None:
        """Should delete a record."""
        store = InMemoryStore()
        await store.save("proj", "key1", {"val": 1})

        await store.delete("proj", "key1")
        result = await store.get("proj", "key1")

        assert result is None

    @pytest.mark.asyncio
    async def test_delete_all(self) -> None:
        """Should delete all records for a projection."""
        store = InMemoryStore()
        await store.save("proj", "key1", {"val": 1})
        await store.save("proj", "key2", {"val": 2})

        await store.delete_all("proj")
        results = await store.get_all("proj")

        assert results == []

    @pytest.mark.asyncio
    async def test_query_with_filters(self) -> None:
        """Should filter records by field values."""
        store = InMemoryStore()
        await store.save("executions", "e1", {"status": "completed", "cost": 10})
        await store.save("executions", "e2", {"status": "failed", "cost": 5})
        await store.save("executions", "e3", {"status": "completed", "cost": 20})

        results = await store.query("executions", filters={"status": "completed"})

        assert len(results) == 2
        assert all(r["status"] == "completed" for r in results)

    @pytest.mark.asyncio
    async def test_query_with_limit_and_offset(self) -> None:
        """Should support pagination via limit and offset."""
        store = InMemoryStore()
        for i in range(5):
            await store.save("items", f"k{i}", {"index": i})

        results = await store.query("items", offset=2, limit=2)

        assert len(results) == 2

    @pytest.mark.asyncio
    async def test_get_by_prefix(self) -> None:
        """Should return records whose keys match a prefix."""
        store = InMemoryStore()
        await store.save("proj", "abc-1", {"val": 1})
        await store.save("proj", "abc-2", {"val": 2})
        await store.save("proj", "xyz-1", {"val": 3})

        results = await store.get_by_prefix("proj", "abc")

        assert len(results) == 2
        keys = [k for k, _ in results]
        assert "abc-1" in keys
        assert "abc-2" in keys

    @pytest.mark.asyncio
    async def test_projection_namespacing(self) -> None:
        """Records in different projections should be independent."""
        store = InMemoryStore()
        await store.save("proj_a", "key1", {"source": "a"})
        await store.save("proj_b", "key1", {"source": "b"})

        a = await store.get("proj_a", "key1")
        b = await store.get("proj_b", "key1")

        assert a == {"source": "a"}
        assert b == {"source": "b"}


class TestProjectionReadStoreProtocol:
    """Tests for ProjectionReadStore protocol."""

    def test_protocol_runtime_checkable(self) -> None:
        """ProjectionReadStore should be runtime checkable."""
        store = InMemoryStore()
        assert isinstance(store, ProjectionReadStore)

    def test_read_only_class_satisfies_protocol(self) -> None:
        """A class with only get/get_all should satisfy ProjectionReadStore."""

        class ReadOnlyStore:
            async def get(self, projection: str, key: str) -> dict[str, Any] | None:
                return None

            async def get_all(self, projection: str) -> list[dict[str, Any]]:
                return []

        store = ReadOnlyStore()
        assert isinstance(store, ProjectionReadStore)
        assert not isinstance(store, ProjectionStore)
