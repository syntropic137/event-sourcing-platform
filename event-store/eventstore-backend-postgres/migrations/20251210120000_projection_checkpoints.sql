-- Migration: Create projection_checkpoints table
-- Date: 2025-12-10
-- Description: Stores checkpoint positions for each projection (ADR-014)
--
-- This table enables:
-- 1. Per-projection position tracking
-- 2. Independent projection rebuilds
-- 3. Atomic updates with projection data (same transaction)

CREATE TABLE IF NOT EXISTS projection_checkpoints (
    -- Unique identifier for the projection (e.g., "order_summary", "user_analytics")
    projection_name TEXT PRIMARY KEY,

    -- Last successfully processed global nonce from the event store
    global_position BIGINT NOT NULL,

    -- Timestamp of last checkpoint update
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Projection schema version (increment to trigger rebuild)
    version INTEGER NOT NULL DEFAULT 1
);

-- Index for efficient lookup by position (useful for monitoring/debugging)
CREATE INDEX IF NOT EXISTS idx_projection_checkpoints_position
    ON projection_checkpoints (global_position);

-- Comment for documentation
COMMENT ON TABLE projection_checkpoints IS
    'Stores checkpoint positions for projections. See ADR-014.';

COMMENT ON COLUMN projection_checkpoints.projection_name IS
    'Unique identifier for the projection (e.g., "order_summary")';

COMMENT ON COLUMN projection_checkpoints.global_position IS
    'Last successfully processed global nonce from the event store';

COMMENT ON COLUMN projection_checkpoints.updated_at IS
    'Timestamp of last checkpoint update';

COMMENT ON COLUMN projection_checkpoints.version IS
    'Projection schema version - increment to trigger rebuild';
