"""
Projection support for building read models from event streams.

Projections transform event streams into queryable read models optimized for specific use cases.

This module re-exports the checkpoint-based projection architecture (ADR-014).
All projections should inherit from CheckpointedProjection.
"""

# Re-export all checkpoint types from the canonical location
from event_sourcing.core.checkpoint import (
    AutoDispatchProjection,
    CheckpointedProjection,
    ProjectionCheckpoint,
    ProjectionCheckpointStore,
    ProjectionResult,
)

__all__ = [
    "AutoDispatchProjection",
    "CheckpointedProjection",
    "ProjectionCheckpoint",
    "ProjectionCheckpointStore",
    "ProjectionResult",
]
