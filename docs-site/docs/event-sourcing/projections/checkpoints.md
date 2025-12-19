# Projection Checkpoints

Checkpoints track which events have been processed by each projection.

## Why Checkpoints?

Without checkpoints:
- ❌ After restart, no way to know which events were processed
- ❌ Events would be replayed from the beginning every time
- ❌ Or events would be skipped, causing data loss

With checkpoints:
- ✅ Projections resume from where they left off
- ✅ At-least-once delivery guaranteed
- ✅ Each projection tracks its own position independently

## Checkpoint Stores

### In-Memory (Testing)

```typescript
import { MemoryCheckpointStore } from '@event-sourcing-platform/typescript/projections';

const checkpointStore = new MemoryCheckpointStore();
```

Good for:
- Unit tests
- Development
- Prototyping

**Warning:** Data is lost on restart.

### PostgreSQL (Production)

```typescript
import { Pool } from 'pg';
import { PostgresCheckpointStore } from '@event-sourcing-platform/typescript/projections';

const pool = new Pool({ connectionString: process.env.DATABASE_URL });

const checkpointStore = new PostgresCheckpointStore({
  client: pool,
  tableName: 'projection_checkpoints', // optional
  schemaName: 'public', // optional
});

// Create table if it doesn't exist
await checkpointStore.ensureTable();
```

The table schema:

```sql
CREATE TABLE projection_checkpoints (
  projection_name VARCHAR(255) PRIMARY KEY,
  global_position BIGINT NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  version INTEGER NOT NULL DEFAULT 1
);
```

## Checkpoint Flow

```
1. Coordinator reads event from store
   │
2. Coordinator checks projection checkpoint
   │ Is event position > checkpoint position?
   ├── No: Skip (already processed)
   └── Yes: Continue
   │
3. Projection processes event
   │
4. On SUCCESS: Projection saves checkpoint
   │            (position = event.globalNonce)
   │
5. On FAILURE: Checkpoint NOT saved
              (event will be retried or DLQ'd)
```

## Saving Checkpoints

In your projection's `processEvent`:

```typescript
protected async processEvent(
  envelope: EventEnvelope,
  checkpointStore: ProjectionCheckpointStore
): Promise<ProjectionResult> {
  try {
    // Your event handling logic
    await this.updateReadModel(envelope);

    // MUST save checkpoint on success
    await this.saveCheckpoint(
      checkpointStore,
      envelope.metadata.globalNonce!
    );

    return ProjectionResult.SUCCESS;
  } catch (error) {
    // Checkpoint NOT saved - will retry
    return ProjectionResult.RETRY;
  }
}
```

## Atomic Checkpoints (SQL)

For projections that write to the same database as checkpoints, use transactions:

```typescript
protected async processEvent(
  envelope: EventEnvelope,
  checkpointStore: ProjectionCheckpointStore
): Promise<ProjectionResult> {
  const client = await this.pool.connect();
  
  try {
    await client.query('BEGIN');

    // Update projection data
    await client.query(
      'INSERT INTO order_summary (id, status) VALUES ($1, $2) ON CONFLICT (id) DO UPDATE SET status = $2',
      [envelope.metadata.aggregateId, 'created']
    );

    // Update checkpoint in same transaction
    await client.query(
      `INSERT INTO projection_checkpoints (projection_name, global_position, updated_at, version)
       VALUES ($1, $2, NOW(), $3)
       ON CONFLICT (projection_name) DO UPDATE SET
         global_position = $2, updated_at = NOW()`,
      [this.getName(), envelope.metadata.globalNonce, this.getVersion()]
    );

    await client.query('COMMIT');
    return ProjectionResult.SUCCESS;

  } catch (error) {
    await client.query('ROLLBACK');
    throw error;
  } finally {
    client.release();
  }
}
```

## Per-Projection vs Global Checkpoints

This platform uses **per-projection checkpoints**:

| Approach | Pros | Cons |
|----------|------|------|
| Global (single position) | Simpler | One slow projection blocks all |
| Per-projection | Independent progress | More storage, more complexity |

We chose per-projection because:
- Projections have different needs (some are slow, some fast)
- One failing projection shouldn't block others
- Independent rebuild capability

## Checkpoint Store Interface

```typescript
interface ProjectionCheckpointStore {
  getCheckpoint(projectionName: string): Promise<ProjectionCheckpoint | null>;
  saveCheckpoint(checkpoint: ProjectionCheckpoint): Promise<void>;
  deleteCheckpoint(projectionName: string): Promise<void>;
  getAllCheckpoints(): Promise<ProjectionCheckpoint[]>;
  getMinimumPosition(): Promise<number>;
}
```

Implement this interface for custom storage backends (Redis, MongoDB, etc.).

## Versioning and Rebuilds

The checkpoint includes a `version` field:

```typescript
interface ProjectionCheckpoint {
  projectionName: string;
  globalPosition: number;
  updatedAt: Date;
  version: number;  // Projection schema version
}
```

When you change projection logic:
1. Increment `getVersion()` in your projection
2. Delete the checkpoint
3. Clear projection data
4. Coordinator will replay from position 0

See [Rebuilding](./rebuilding.md) for details.
