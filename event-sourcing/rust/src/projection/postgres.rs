//! Postgres projection store: read model and checkpoint in one transaction.

use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Row, Transaction};

use super::{fence_error, CheckpointKey, CheckpointStore, ProjectionStore};
use crate::error::{Error, Result};

const DDL: &str = r#"
CREATE TABLE IF NOT EXISTS esp_projection_checkpoints (
    tenant_id          TEXT        NOT NULL,
    projection_name    TEXT        NOT NULL,
    projection_version INTEGER     NOT NULL,
    feed               TEXT        NOT NULL DEFAULT '',
    global_position    BIGINT      NOT NULL,
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, projection_name, projection_version, feed)
)"#;

// Advance only forward; a row that is already at or past `position` is left
// alone, which the caller turns into a fencing error.
const UPSERT: &str = r#"
INSERT INTO esp_projection_checkpoints
    (tenant_id, projection_name, projection_version, feed, global_position, updated_at)
VALUES ($1, $2, $3, $4, $5, NOW())
ON CONFLICT (tenant_id, projection_name, projection_version, feed) DO UPDATE
    SET global_position = EXCLUDED.global_position, updated_at = NOW()
    WHERE esp_projection_checkpoints.global_position < EXCLUDED.global_position"#;

const SELECT: &str = r#"
SELECT global_position FROM esp_projection_checkpoints
WHERE tenant_id = $1 AND projection_name = $2 AND projection_version = $3 AND feed = $4"#;

const DELETE: &str = r#"
DELETE FROM esp_projection_checkpoints
WHERE tenant_id = $1 AND projection_name = $2 AND projection_version = $3 AND feed = $4"#;

fn db(err: sqlx::Error) -> Error {
    Error::Repository(anyhow::Error::new(err))
}

fn to_i64(position: u64) -> Result<i64> {
    i64::try_from(position)
        .map_err(|_| Error::Repository(anyhow::anyhow!("position {position} exceeds BIGINT")))
}

/// Postgres-backed [`ProjectionStore`] and [`CheckpointStore`].
///
/// As a `ProjectionStore`, `Tx` is a `sqlx` transaction: the projection
/// writes its read-model tables through it and the runner adds the
/// checkpoint upsert to the same transaction before committing. Read model
/// and checkpoint therefore advance together or not at all.
///
/// Checkpoints live in `esp_projection_checkpoints`, keyed by tenant,
/// projection name, projection version, and feed. Call [`migrate`] once.
///
/// [`migrate`]: PostgresProjectionStore::migrate
#[derive(Debug, Clone)]
pub struct PostgresProjectionStore {
    pool: PgPool,
}

impl PostgresProjectionStore {
    /// Use an existing pool (typically the read model's database).
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// The pool, for read-model queries.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Create the checkpoint table if it does not exist.
    pub async fn migrate(&self) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        // Serialize concurrent migrations (CREATE TABLE IF NOT EXISTS can race).
        sqlx::query("SELECT pg_advisory_xact_lock(7302113537)")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query(DDL).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)
    }

    async fn upsert<'e, E>(&self, exec: E, key: &CheckpointKey, position: u64) -> Result<u64>
    where
        E: sqlx::Executor<'e, Database = Postgres>,
    {
        Ok(sqlx::query(UPSERT)
            .bind(&key.tenant_id)
            .bind(&key.projection_name)
            .bind(key.projection_version as i32)
            .bind(&key.feed)
            .bind(to_i64(position)?)
            .execute(exec)
            .await
            .map_err(db)?
            .rows_affected())
    }

    async fn stored(&self, key: &CheckpointKey) -> Result<Option<u64>> {
        let row = sqlx::query(SELECT)
            .bind(&key.tenant_id)
            .bind(&key.projection_name)
            .bind(key.projection_version as i32)
            .bind(&key.feed)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        Ok(row.map(|r| r.get::<i64, _>("global_position") as u64))
    }
}

#[async_trait]
impl ProjectionStore for PostgresProjectionStore {
    type Tx = Transaction<'static, Postgres>;

    async fn load_checkpoint(&self, key: &CheckpointKey) -> Result<Option<u64>> {
        self.stored(key).await
    }

    async fn begin(&self, _key: &CheckpointKey) -> Result<Self::Tx> {
        self.pool.begin().await.map_err(db)
    }

    async fn commit(&self, mut tx: Self::Tx, key: &CheckpointKey, position: u64) -> Result<()> {
        if self.upsert(&mut *tx, key, position).await? == 0 {
            // Dropping `tx` rolls back the projection's writes too.
            drop(tx);
            let stored = self.stored(key).await?.unwrap_or(0);
            return Err(fence_error(key, stored, position));
        }
        tx.commit().await.map_err(db)
    }

    async fn delete_checkpoint(&self, key: &CheckpointKey) -> Result<()> {
        CheckpointStore::delete(self, key).await
    }
}

#[async_trait]
impl CheckpointStore for PostgresProjectionStore {
    async fn load(&self, key: &CheckpointKey) -> Result<Option<u64>> {
        self.stored(key).await
    }

    async fn save(&self, key: &CheckpointKey, position: u64) -> Result<()> {
        if self.upsert(&self.pool, key, position).await? == 0 {
            let stored = self.stored(key).await?.unwrap_or(0);
            return Err(fence_error(key, stored, position));
        }
        Ok(())
    }

    async fn delete(&self, key: &CheckpointKey) -> Result<()> {
        sqlx::query(DELETE)
            .bind(&key.tenant_id)
            .bind(&key.projection_name)
            .bind(key.projection_version as i32)
            .bind(&key.feed)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }
}
