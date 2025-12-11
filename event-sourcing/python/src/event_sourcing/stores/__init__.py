"""
Checkpoint stores for projection position tracking.

This module provides implementations of ProjectionCheckpointStore:
- PostgresCheckpointStore: Production-ready, ACID-compliant storage
- MemoryCheckpointStore: For unit tests only (test environment enforced)
"""

from event_sourcing.stores.memory_checkpoint import MemoryCheckpointStore
from event_sourcing.stores.postgres_checkpoint import PostgresCheckpointStore

__all__ = [
    "PostgresCheckpointStore",
    "MemoryCheckpointStore",
]
