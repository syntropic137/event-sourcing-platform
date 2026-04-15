"""
Store implementations for projection checkpoints and read-model data.

Checkpoint stores (ProjectionCheckpointStore):
- PostgresCheckpointStore: Production-ready, ACID-compliant storage
- MemoryCheckpointStore: For unit tests only (test environment enforced)

Projection stores (ProjectionStore):
- MemoryProjectionStore: For unit tests only (test environment enforced)
"""

from event_sourcing.stores.memory_checkpoint import MemoryCheckpointStore
from event_sourcing.stores.memory_projection import MemoryProjectionStore
from event_sourcing.stores.postgres_checkpoint import PostgresCheckpointStore

__all__ = [
    "MemoryCheckpointStore",
    "MemoryProjectionStore",
    "PostgresCheckpointStore",
]
