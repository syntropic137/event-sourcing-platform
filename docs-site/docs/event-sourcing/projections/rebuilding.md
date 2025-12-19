# Rebuilding Projections

How to rebuild projections from scratch.

## When to Rebuild

Rebuild a projection when:
- **Schema changed** — Projection logic was updated
- **Bug fixed** — Need to reprocess with corrected logic
- **Data corrupted** — Projection data is inconsistent
- **New projection** — Starting fresh

## Rebuild Process

```
1. Delete checkpoint
   └── Projection forgets its position

2. Clear projection data
   └── Remove existing read model

3. Coordinator restarts from position 0
   └── Replays all events through projection

4. Projection rebuilds read model
   └── Fresh data from event history
```

## Basic Rebuild

```typescript
// Using coordinator
await coordinator.rebuildProjection('order-summary');

// Or manually
await checkpointStore.deleteCheckpoint('order-summary');
await projection.clearData();
// Restart coordinator to begin replay
```

## Full Rebuild Flow

```typescript
async function rebuildProjection(projectionName: string) {
  const projection = projections.get(projectionName);
  if (!projection) throw new Error('Projection not found');

  console.log(`Starting rebuild of ${projectionName}`);

  // 1. Stop processing (if coordinator is running)
  await coordinator.stop();

  // 2. Delete checkpoint
  await checkpointStore.deleteCheckpoint(projectionName);
  console.log('Checkpoint deleted');

  // 3. Clear projection data
  await projection.clearData();
  console.log('Projection data cleared');

  // 4. Restart coordinator
  await coordinator.start();
  console.log('Rebuild in progress...');

  // 5. Wait for catch-up (optional)
  await waitForCatchUp(projectionName);
  console.log('Rebuild complete');
}

async function waitForCatchUp(projectionName: string) {
  while (true) {
    const health = await healthChecker.getHealth(projectionName);
    if (health.lag === 0) break;
    console.log(`Catching up... lag: ${health.lag}`);
    await sleep(1000);
  }
}
```

## Versioned Rebuilds

Use version numbers to detect when rebuild is needed:

```typescript
class OrderSummaryProjection extends CheckpointedProjection {
  getName() { return 'order-summary'; }
  
  getVersion() {
    return 2; // Increment when logic changes
  }
}
```

Check on startup:

```typescript
async function checkProjectionVersions() {
  for (const projection of projections) {
    const checkpoint = await checkpointStore.getCheckpoint(projection.getName());
    
    if (checkpoint && checkpoint.version !== projection.getVersion()) {
      console.log(
        `Projection ${projection.getName()} version mismatch: ` +
        `stored=${checkpoint.version}, current=${projection.getVersion()}`
      );
      await rebuildProjection(projection.getName());
    }
  }
}
```

## Implementing clearData()

Override `clearData()` in your projection:

```typescript
class OrderSummaryProjection extends CheckpointedProjection {
  private db: Database;

  async clearData(): Promise<void> {
    // Clear projection tables
    await this.db.query('TRUNCATE TABLE order_summary');
    await this.db.query('TRUNCATE TABLE order_items');
    
    // Clear caches
    this.cache.clear();
    
    // Reset in-memory state
    this.orders.clear();
  }
}
```

## Partial Rebuilds

Sometimes you only need to reprocess from a specific point:

```typescript
async function partialRebuild(projectionName: string, fromPosition: number) {
  // Set checkpoint to just before target position
  await checkpointStore.saveCheckpoint({
    projectionName,
    globalPosition: fromPosition - 1,
    updatedAt: new Date(),
    version: projection.getVersion(),
  });

  // Note: This doesn't clear existing data!
  // Only works if projection can handle re-processing
}
```

**Warning:** Partial rebuilds only work if your projection is idempotent.

## Rebuild Without Downtime

For zero-downtime rebuilds:

```typescript
// 1. Create new projection with different name
class OrderSummaryV2Projection extends CheckpointedProjection {
  getName() { return 'order-summary-v2'; }
  // ... new logic
}

// 2. Add to coordinator alongside old one
const coordinator = new SubscriptionCoordinator({
  projections: [
    orderSummaryV1,  // Keep serving reads
    orderSummaryV2,  // Building in background
  ],
});

// 3. Wait for v2 to catch up

// 4. Switch traffic to v2

// 5. Remove v1 and rename v2 (optional)
```

## Rebuild Monitoring

Track rebuild progress:

```typescript
coordinator.callbacks = {
  onEventProcessed: (projectionName, envelope, result) => {
    if (isRebuilding[projectionName]) {
      const health = healthChecker.getHealth(projectionName);
      const progress = (health.lastProcessedPosition / health.currentHeadPosition) * 100;
      console.log(`Rebuild ${projectionName}: ${progress.toFixed(1)}%`);
    }
  },
};
```

## Rebuild Best Practices

### 1. Schedule During Low Traffic

```typescript
// Run rebuilds during off-peak hours
if (isOffPeakHours()) {
  await rebuildProjection('analytics-projection');
}
```

### 2. Batch Database Operations

```typescript
class OrderSummaryProjection extends CheckpointedProjection {
  private batch: OrderSummary[] = [];
  private batchSize = 100;

  protected async processEvent(envelope, checkpointStore) {
    this.batch.push(this.mapToSummary(envelope));

    if (this.batch.length >= this.batchSize) {
      await this.flushBatch();
    }

    await this.saveCheckpoint(checkpointStore, envelope.metadata.globalNonce!);
    return ProjectionResult.SUCCESS;
  }

  private async flushBatch() {
    await this.db.batchInsert(this.batch);
    this.batch = [];
  }
}
```

### 3. Estimate Rebuild Time

```typescript
async function estimateRebuildTime(projectionName: string): Promise<number> {
  const currentHead = await eventStore.getCurrentPosition();
  const eventsPerSecond = 1000; // Estimate based on your system
  
  return currentHead / eventsPerSecond; // Seconds
}

const seconds = await estimateRebuildTime('order-summary');
console.log(`Estimated rebuild time: ${Math.round(seconds / 60)} minutes`);
```

### 4. Test Rebuilds in Staging

Always test rebuild procedures in a staging environment before production.
