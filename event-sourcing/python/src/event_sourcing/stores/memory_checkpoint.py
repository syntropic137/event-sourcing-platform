"""
In-memory implementation of ProjectionCheckpointStore.

This is for TEST ENVIRONMENTS ONLY (per ADR-004).
"""

import os

from event_sourcing.core.checkpoint import (
    ProjectionCheckpoint,
    ProjectionCheckpointStore,
)


def _assert_test_environment() -> None:
    """
    Ensure we are running in a test environment.

    Per ADR-004, mock/in-memory implementations must validate they are
    running in the test environment. This prevents accidental use in
    production where data would be lost on restart.

    Raises:
        RuntimeError: If not in a test environment
    """
    # Check for common test environment indicators
    is_test = (
        os.getenv("PYTEST_CURRENT_TEST") is not None
        or os.getenv("TEST_ENV") == "true"
        or os.getenv("IS_TEST") == "true"
        or os.getenv("NODE_ENV") == "test"
        or os.getenv("ENVIRONMENT") == "test"
    )

    if not is_test:
        raise RuntimeError(
            "MemoryCheckpointStore is for test environments only. "
            "Set TEST_ENV=true or use PostgresCheckpointStore for production. "
            "See ADR-004 for details."
        )


class MemoryCheckpointStore:
    """
    In-memory checkpoint store for unit tests.

    ⚠️  TEST ENVIRONMENT ONLY - Will raise RuntimeError if used outside tests.

    This implementation stores checkpoints in a dictionary and is lost
    when the process exits. Use PostgresCheckpointStore for production.

    Usage (in tests only):
        store = MemoryCheckpointStore()

        await store.save_checkpoint(
            ProjectionCheckpoint(
                projection_name="test_projection",
                global_position=42,
                updated_at=datetime.now(UTC),
            )
        )

        checkpoint = await store.get_checkpoint("test_projection")
    """

    def __init__(self) -> None:
        """Initialize the memory checkpoint store."""
        _assert_test_environment()
        self._checkpoints: dict[str, ProjectionCheckpoint] = {}

    async def get_checkpoint(
        self, projection_name: str
    ) -> ProjectionCheckpoint | None:
        """
        Get the current checkpoint for a projection.

        Args:
            projection_name: Unique identifier for the projection

        Returns:
            Current checkpoint or None if no checkpoint exists
        """
        return self._checkpoints.get(projection_name)

    async def save_checkpoint(self, checkpoint: ProjectionCheckpoint) -> None:
        """
        Save a checkpoint.

        Args:
            checkpoint: Checkpoint to save
        """
        self._checkpoints[checkpoint.projection_name] = checkpoint

    async def delete_checkpoint(self, projection_name: str) -> None:
        """
        Delete a checkpoint.

        Args:
            projection_name: Unique identifier for the projection
        """
        self._checkpoints.pop(projection_name, None)

    async def get_all_checkpoints(self) -> list[ProjectionCheckpoint]:
        """
        Get all checkpoints.

        Returns:
            List of all stored checkpoints
        """
        return list(self._checkpoints.values())

    def clear(self) -> None:
        """Clear all checkpoints (test utility)."""
        self._checkpoints.clear()


# Protocol compliance assertion (static check)
# MemoryCheckpointStore implements ProjectionCheckpointStore
_: type[ProjectionCheckpointStore] = MemoryCheckpointStore
