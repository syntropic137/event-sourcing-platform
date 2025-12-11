"""
PostgreSQL implementation of ProjectionCheckpointStore.

This provides ACID-compliant checkpoint persistence for projections.
Checkpoints are stored in a dedicated `projection_checkpoints` table.

See ADR-014 for architectural rationale.
"""

import logging
from typing import Any

from event_sourcing.core.checkpoint import (
    ProjectionCheckpoint,
    ProjectionCheckpointStore,
)

logger = logging.getLogger(__name__)


class PostgresCheckpointStore:
    """
    PostgreSQL-backed checkpoint store for production use.

    This implementation:
    - Uses upsert (INSERT ... ON CONFLICT UPDATE) for atomic saves
    - Supports integration with the same transaction as projection updates
    - Provides structured logging for observability

    Usage:
        # With asyncpg connection pool
        store = PostgresCheckpointStore(pool)

        # Get checkpoint
        checkpoint = await store.get_checkpoint("my_projection")

        # Save checkpoint (upsert)
        await store.save_checkpoint(
            ProjectionCheckpoint(
                projection_name="my_projection",
                global_position=42,
                updated_at=datetime.now(UTC),
                version=1,
            )
        )

    For atomic updates with projection data, use the same connection:
        async with pool.acquire() as conn:
            async with conn.transaction():
                # Update projection data
                await conn.execute("UPDATE my_table SET ...")

                # Update checkpoint in same transaction
                await store.save_checkpoint_with_connection(conn, checkpoint)
    """

    # SQL statements
    CREATE_TABLE_SQL = """
        CREATE TABLE IF NOT EXISTS projection_checkpoints (
            projection_name TEXT PRIMARY KEY,
            global_position BIGINT NOT NULL,
            updated_at TIMESTAMPTZ NOT NULL,
            version INTEGER NOT NULL DEFAULT 1
        );
    """

    GET_CHECKPOINT_SQL = """
        SELECT projection_name, global_position, updated_at, version
        FROM projection_checkpoints
        WHERE projection_name = $1;
    """

    SAVE_CHECKPOINT_SQL = """
        INSERT INTO projection_checkpoints (projection_name, global_position, updated_at, version)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (projection_name) DO UPDATE SET
            global_position = EXCLUDED.global_position,
            updated_at = EXCLUDED.updated_at,
            version = EXCLUDED.version;
    """

    DELETE_CHECKPOINT_SQL = """
        DELETE FROM projection_checkpoints
        WHERE projection_name = $1;
    """

    GET_ALL_CHECKPOINTS_SQL = """
        SELECT projection_name, global_position, updated_at, version
        FROM projection_checkpoints
        ORDER BY projection_name;
    """

    def __init__(self, pool: Any) -> None:
        """
        Initialize the PostgreSQL checkpoint store.

        Args:
            pool: asyncpg connection pool or compatible interface
        """
        self._pool = pool
        self._table_created = False

    async def _ensure_table(self) -> None:
        """Ensure the checkpoint table exists (lazy creation)."""
        if self._table_created:
            return

        async with self._pool.acquire() as conn:
            await conn.execute(self.CREATE_TABLE_SQL)
            self._table_created = True
            logger.info("Ensured projection_checkpoints table exists")

    async def get_checkpoint(self, projection_name: str) -> ProjectionCheckpoint | None:
        """
        Get the current checkpoint for a projection.

        Args:
            projection_name: Unique identifier for the projection

        Returns:
            Current checkpoint or None if no checkpoint exists
        """
        await self._ensure_table()

        async with self._pool.acquire() as conn:
            row = await conn.fetchrow(self.GET_CHECKPOINT_SQL, projection_name)

        if row is None:
            logger.debug(
                "No checkpoint found",
                extra={"projection_name": projection_name},
            )
            return None

        checkpoint = ProjectionCheckpoint(
            projection_name=row["projection_name"],
            global_position=row["global_position"],
            updated_at=row["updated_at"],
            version=row["version"],
        )

        logger.debug(
            "Retrieved checkpoint",
            extra={
                "projection_name": projection_name,
                "global_position": checkpoint.global_position,
            },
        )
        return checkpoint

    async def save_checkpoint(self, checkpoint: ProjectionCheckpoint) -> None:
        """
        Save a checkpoint atomically using upsert.

        Args:
            checkpoint: Checkpoint to save
        """
        await self._ensure_table()

        async with self._pool.acquire() as conn:
            await self.save_checkpoint_with_connection(conn, checkpoint)

    async def save_checkpoint_with_connection(
        self,
        conn: Any,
        checkpoint: ProjectionCheckpoint,
    ) -> None:
        """
        Save a checkpoint using an existing connection.

        Use this method when you need atomic updates with projection data
        within the same transaction.

        Args:
            conn: asyncpg connection (within a transaction)
            checkpoint: Checkpoint to save
        """
        await conn.execute(
            self.SAVE_CHECKPOINT_SQL,
            checkpoint.projection_name,
            checkpoint.global_position,
            checkpoint.updated_at,
            checkpoint.version,
        )

        logger.debug(
            "Saved checkpoint",
            extra={
                "projection_name": checkpoint.projection_name,
                "global_position": checkpoint.global_position,
                "version": checkpoint.version,
            },
        )

    async def delete_checkpoint(self, projection_name: str) -> None:
        """
        Delete a checkpoint (used for projection rebuilds).

        Args:
            projection_name: Unique identifier for the projection
        """
        await self._ensure_table()

        async with self._pool.acquire() as conn:
            await conn.execute(self.DELETE_CHECKPOINT_SQL, projection_name)

        logger.info(
            "Deleted checkpoint",
            extra={"projection_name": projection_name},
        )

    async def get_all_checkpoints(self) -> list[ProjectionCheckpoint]:
        """
        Get all checkpoints (used by SubscriptionCoordinator).

        Returns:
            List of all stored checkpoints
        """
        await self._ensure_table()

        async with self._pool.acquire() as conn:
            rows = await conn.fetch(self.GET_ALL_CHECKPOINTS_SQL)

        checkpoints = [
            ProjectionCheckpoint(
                projection_name=row["projection_name"],
                global_position=row["global_position"],
                updated_at=row["updated_at"],
                version=row["version"],
            )
            for row in rows
        ]

        logger.debug(
            "Retrieved all checkpoints",
            extra={"count": len(checkpoints)},
        )
        return checkpoints


# Protocol compliance assertion (static check)
# PostgresCheckpointStore implements ProjectionCheckpointStore
_: type[ProjectionCheckpointStore] = PostgresCheckpointStore
