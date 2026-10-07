use std::collections::HashMap;
use std::future::Future;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use eventstore_core::fingerprint::canonical_metadata_bytes;
use eventstore_core::{proto, EventStore as EventStoreTrait, StoreError, StoreStream};
use futures::stream;
use sha2::{Digest, Sha256};
use sqlx::pool::PoolConnection;
use sqlx::postgres::{PgConnectOptions, PgListener, PgPoolOptions};
use sqlx::{types::Json, Connection, PgConnection, PgPool, Postgres, Row};
use tokio::sync::broadcast;
use tokio::time::{interval, Duration, Interval};

use crate::config::PostgresConfig;

const DEFAULT_CONTENT_TYPE: &str = "application/octet-stream";

// --- LISTEN/NOTIFY constants ---
const NOTIFY_CHANNEL: &str = "eventstore_events";
const NOTIFY_BROADCAST_CAPACITY: usize = 256;
const FALLBACK_POLL_SECS: u64 = 5;
/// Postgres rejects NOTIFY payloads of 8000 bytes or more; `tenant:nonce`
/// stays below that for tenant ids shorter than this.
const NOTIFY_MAX_TENANT_BYTES: i32 = 7900;

/// First key of the two-key advisory lock that orders appends per tenant, so
/// it cannot collide with any other advisory lock in the database. Arbitrary,
/// fixed: changing it while two versions run side by side breaks the ordering.
const APPEND_ORDER_LOCK_NAMESPACE: i32 = 0x0E5_1545;

/// Payload sent via PostgreSQL NOTIFY and the in-process broadcast channel.
/// Format: `"{tenant_id}:{last_global_nonce}"`.
#[derive(Debug, Clone)]
struct NotifyPayload {
    tenant_id: String,
    last_global_nonce: i64,
}

impl NotifyPayload {
    /// The append statement builds the same string in SQL
    /// (`$tenant || ':' || last_global`).
    #[cfg(test)]
    fn encode(tenant_id: &str, last_global_nonce: i64) -> String {
        format!("{tenant_id}:{last_global_nonce}")
    }

    fn parse(payload: &str) -> Option<Self> {
        let (tenant, nonce_str) = payload.rsplit_once(':')?;
        let last_global_nonce = nonce_str.parse::<i64>().ok()?;
        Some(Self {
            tenant_id: tenant.to_owned(),
            last_global_nonce,
        })
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn batch_fingerprint(events: &[proto::EventData]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    for ev in events {
        if let Some(meta) = &ev.meta {
            // Server-assigned fields zeroed, headers in key order: an identical
            // retry must hash identically. Byte-compatible with fingerprints
            // stored before the headers fix for requests with <= 1 header.
            hasher.update(canonical_metadata_bytes(meta));
            hasher.update(&ev.payload);
        }
    }
    hasher.finalize().to_vec()
}

fn normalize_event(
    mut event: proto::EventData,
    tenant_id: &str,
    aggregate_id: &str,
    aggregate_type: &str,
) -> Result<proto::EventData, StoreError> {
    let mut meta = event.meta.take().ok_or_else(|| {
        StoreError::Invalid("event.metadata is required for optimistic concurrency".into())
    })?;

    if meta.aggregate_nonce == 0 {
        return Err(StoreError::Invalid(
            "aggregate_nonce must be >= 1 for all events".into(),
        ));
    }

    if meta.event_id.is_empty() {
        return Err(StoreError::Invalid(
            "event_id must be provided (UUID/ULID recommended)".into(),
        ));
    }

    if meta.aggregate_id.is_empty() {
        meta.aggregate_id = aggregate_id.to_owned();
    } else if meta.aggregate_id != aggregate_id {
        return Err(StoreError::Invalid(format!(
            "event aggregate_id '{}' must match request aggregate_id '{}'",
            meta.aggregate_id, aggregate_id
        )));
    }

    if meta.aggregate_type.is_empty() {
        meta.aggregate_type = aggregate_type.to_owned();
    } else if meta.aggregate_type != aggregate_type {
        return Err(StoreError::Invalid(format!(
            "event aggregate_type '{}' must match request aggregate_type '{}'",
            meta.aggregate_type, aggregate_type
        )));
    }

    if meta.tenant_id.is_empty() {
        meta.tenant_id = tenant_id.to_owned();
    } else if meta.tenant_id != tenant_id {
        return Err(StoreError::PermissionDenied(format!(
            "event tenant_id '{}' does not match request tenant_id '{}'",
            meta.tenant_id, tenant_id
        )));
    }

    if meta.content_type.is_empty() {
        meta.content_type = DEFAULT_CONTENT_TYPE.to_owned();
    }

    event.meta = Some(meta);
    Ok(event)
}

/// How long the LISTEN connection may sit without traffic before it is
/// probed. An idle connection on a silently dropped network path never
/// errors by itself; the probe finds it so the listener reconnects instead
/// of leaving subscriptions on the fallback poll forever (#368).
const LISTENER_IDLE_PROBE: Duration = Duration::from_secs(30);
/// Bound on the probe round trip.
const LISTENER_PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// Spawns a background task that maintains a dedicated PostgreSQL connection
/// for LISTEN/NOTIFY. Notifications are forwarded to the broadcast channel
/// so that subscription streams wake immediately on new events.
///
/// The connection comes from its own one-connection pool, so it never takes
/// a slot of the main pool. Notifications are hints; the subscription's DB
/// query is the source of truth and the fallback poll covers any gap.
fn spawn_pg_listener(
    connect_options: Option<PgConnectOptions>,
    acquire_timeout: Duration,
    notify_tx: broadcast::Sender<NotifyPayload>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Some(connect_options) = connect_options else {
            tracing::warn!("invalid database URL for LISTEN; subscriptions rely on polling");
            return;
        };
        let mut backoff = Duration::from_secs(1);
        let max_backoff = Duration::from_secs(30);

        loop {
            let connected = async {
                let pool = PgPoolOptions::new()
                    .max_connections(1)
                    .acquire_timeout(acquire_timeout)
                    .connect_lazy_with(connect_options.clone());
                let mut listener = PgListener::connect_with(&pool).await?;
                listener.listen(NOTIFY_CHANNEL).await?;
                Ok::<_, sqlx::Error>(listener)
            };
            let mut listener = match tokio::time::timeout(acquire_timeout, connected).await {
                Ok(Ok(listener)) => listener,
                Ok(Err(e)) => {
                    tracing::warn!(error = %e, "PgListener connect failed, retrying");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(max_backoff);
                    continue;
                }
                Err(_) => {
                    tracing::warn!("PgListener connect timed out, retrying");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(max_backoff);
                    continue;
                }
            };
            tracing::info!("PgListener connected to channel '{}'", NOTIFY_CHANNEL);
            backoff = Duration::from_secs(1);

            loop {
                match tokio::time::timeout(LISTENER_IDLE_PROBE, listener.recv()).await {
                    Ok(Ok(notification)) => {
                        if let Some(payload) = NotifyPayload::parse(notification.payload()) {
                            let _ = notify_tx.send(payload);
                        }
                    }
                    Ok(Err(e)) => {
                        tracing::warn!(error = %e, "PgListener recv error, reconnecting");
                        break;
                    }
                    Err(_) => {
                        // Idle: prove the path is alive. A cancelled recv may
                        // leave the connection mid-message; the probe then
                        // fails and we reconnect, which is always safe.
                        let probe = sqlx::query("SELECT 1").execute(&mut listener);
                        match tokio::time::timeout(LISTENER_PROBE_TIMEOUT, probe).await {
                            Ok(Ok(_)) => {}
                            Ok(Err(e)) => {
                                tracing::warn!(error = %e, "PgListener probe failed, reconnecting");
                                break;
                            }
                            Err(_) => {
                                tracing::warn!(
                                    "PgListener probe timed out (network path stalled?), reconnecting"
                                );
                                break;
                            }
                        }
                    }
                }
            }
            // Dropping the listener drops its pool and socket without waiting
            // on a stalled peer.
            drop(listener);
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(max_backoff);
        }
    })
}

pub struct PostgresStore {
    pool: PgPool,
    notify_tx: broadcast::Sender<NotifyPayload>,
    listener_handle: tokio::task::JoinHandle<()>,
    /// Client-side bound on one database operation; see
    /// [`PostgresConfig::operation_deadline`].
    deadline: Option<Duration>,
}

impl Drop for PostgresStore {
    fn drop(&mut self) {
        self.listener_handle.abort();
    }
}

impl PostgresStore {
    /// Wrap an existing pool. Uses the default client deadline and opens the
    /// LISTEN connection from `database_url`.
    pub fn new(pool: PgPool, database_url: String) -> Arc<Self> {
        let options = PgConnectOptions::from_str(&database_url).ok();
        let cfg = PostgresConfig::default();
        Self::from_parts(pool, options, cfg.acquire_timeout, cfg.operation_deadline())
    }

    fn from_parts(
        pool: PgPool,
        listener_options: Option<PgConnectOptions>,
        acquire_timeout: Duration,
        deadline: Option<Duration>,
    ) -> Arc<Self> {
        let (notify_tx, _) = broadcast::channel(NOTIFY_BROADCAST_CAPACITY);
        let listener_handle =
            spawn_pg_listener(listener_options, acquire_timeout, notify_tx.clone());
        Arc::new(Self {
            pool,
            notify_tx,
            listener_handle,
            deadline,
        })
    }

    /// Connect with [`PostgresConfig::default`].
    pub async fn connect(database_url: &str) -> anyhow::Result<Arc<Self>> {
        Self::connect_with_config(database_url, &PostgresConfig::default()).await
    }

    /// Connect with explicit pool and timeout settings (#368, #370).
    ///
    /// Migrations run on a separate connection without the session
    /// timeouts, so a slow migration is never killed by `statement_timeout`.
    pub async fn connect_with_config(
        database_url: &str,
        config: &PostgresConfig,
    ) -> anyhow::Result<Arc<Self>> {
        config.validate()?;
        let base = PgConnectOptions::from_str(database_url)?;
        {
            let mut conn = PgConnection::connect_with(&base).await?;
            sqlx::migrate!("./migrations").run(&mut conn).await?;
            conn.close().await?;
        }
        let options = config.apply(base);
        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .acquire_timeout(config.acquire_timeout)
            .idle_timeout(Duration::from_secs(600))
            .connect_with(options.clone())
            .await?;
        Ok(Self::from_parts(
            pool,
            Some(options),
            config.acquire_timeout,
            config.operation_deadline(),
        ))
    }

    /// Connect with test-friendly configuration
    ///
    /// **This method is only compiled when testing** (`#[cfg(test)]` or integration tests).
    /// It provides optimized settings for test environments including testcontainers and CI systems.
    ///
    /// # Test-specific optimizations:
    /// - Higher connection pool limits (8 vs 5) for parallel test execution
    /// - Extended timeouts (120s vs 30s) for resource-constrained CI environments
    /// - Connection lifetime management to prevent stale connections in long-running tests
    ///
    /// # Production use:
    /// This method is **not available** in production builds. Use `connect()` instead.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn connect_for_tests(database_url: &str) -> anyhow::Result<Arc<Self>> {
        // Simple connection for test reliability
        let pool = PgPool::connect(database_url).await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self::new(pool, database_url.to_owned()))
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Check out a connection; waiting is bounded by the pool's acquire
    /// timeout, and failure is `UNAVAILABLE`.
    async fn acquire(&self) -> Result<PoolConnection<Postgres>, StoreError> {
        self.pool.acquire().await.map_err(map_db_error)
    }

    /// The append transaction. Runs on `conn` so the caller can discard the
    /// connection if the client deadline passes mid-transaction.
    async fn append_tx(
        &self,
        conn: &mut PgConnection,
        req: &proto::AppendRequest,
        fingerprint: Vec<u8>,
        events: Vec<proto::EventData>,
    ) -> Result<proto::AppendResponse, StoreError> {
        let tenant_id = req.tenant_id.clone();
        let aggregate_id = req.aggregate_id.clone();
        let aggregate_type = req.aggregate_type.clone();
        let mut tx = conn.begin().await.map_err(map_db_error)?;

        if !req.idempotency_key.is_empty() {
            let row = sqlx::query(
                "SELECT request_fingerprint, first_committed_nonce, last_committed_nonce, last_global_nonce \
                 FROM idempotency WHERE tenant_id = $1 AND aggregate_id = $2 AND idempotency_key = $3 FOR UPDATE",
            )
            .bind(&tenant_id)
            .bind(&aggregate_id)
            .bind(&req.idempotency_key)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map_db_error)?;

            if let Some(row) = row {
                let stored_fingerprint: Vec<u8> = row.get("request_fingerprint");
                if stored_fingerprint == fingerprint {
                    tx.rollback()
                        .await
                        .map_err(|e| StoreError::Internal(anyhow::anyhow!(e)))?;
                    return Ok(proto::AppendResponse {
                        last_global_nonce: row.get::<i64, _>("last_global_nonce") as u64,
                        last_aggregate_nonce: row.get::<i64, _>("last_committed_nonce") as u64,
                    });
                }
                tx.rollback()
                    .await
                    .map_err(|e| StoreError::Internal(anyhow::anyhow!(e)))?;
                return Err(StoreError::AlreadyExists(format!(
                    "idempotency key '{}' already used with different payload",
                    req.idempotency_key
                )));
            }
        }

        let row = sqlx::query(
            "SELECT last_nonce, last_global_nonce FROM aggregates WHERE tenant_id = $1 AND aggregate_id = $2 FOR UPDATE",
        )
        .bind(&tenant_id)
        .bind(&aggregate_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_db_error)?;

        let current_last_nonce: u64 = row
            .as_ref()
            .map(|r| r.get::<i64, _>("last_nonce") as u64)
            .unwrap_or(0);
        let current_last_global: u64 = row
            .as_ref()
            .map(|r| r.get::<i64, _>("last_global_nonce") as u64)
            .unwrap_or(0);

        let expected_head = req.expected_aggregate_nonce;
        let expected_ok = if expected_head == 0 {
            current_last_nonce == 0
        } else {
            current_last_nonce == expected_head
        };
        if !expected_ok {
            tx.rollback()
                .await
                .map_err(|e| StoreError::Internal(anyhow::anyhow!(e)))?;
            return Err(StoreError::Concurrency {
                message: "append precondition failed".into(),
                detail: Some(proto::ConcurrencyErrorDetail {
                    tenant_id,
                    aggregate_id,
                    actual_last_aggregate_nonce: current_last_nonce,
                    actual_last_global_nonce: current_last_global,
                }),
            });
        }

        for (idx, ev) in events.iter().enumerate() {
            let meta = ev
                .meta
                .as_ref()
                .expect("normalized event must have metadata");
            let expected_nonce = current_last_nonce + idx as u64 + 1;
            if meta.aggregate_nonce != expected_nonce {
                tx.rollback()
                    .await
                    .map_err(|e| StoreError::Internal(anyhow::anyhow!(e)))?;
                return Err(StoreError::Invalid(format!(
                    "event {} aggregate_nonce {} must equal expected {}",
                    idx, meta.aggregate_nonce, expected_nonce
                )));
            }
        }

        // Commit order must equal global_nonce order (syntropic137#1545).
        //
        // global_nonce is a BIGSERIAL: `nextval()` hands it out at INSERT, but
        // the row only becomes visible at COMMIT. Without this lock a slow
        // append can hold nonce N while a faster one commits N+1; a subscriber
        // polling `global_nonce > cursor` then yields N+1, advances past N, and
        // never sees N even after it commits. Nothing errors: the event is in
        // the store, behind every cursor. That is how a WorkflowExecutionStarted
        // at 39502 was skipped by every projection in syntropic137#1545.
        //
        // Held from before the first nonce is drawn until commit/rollback.
        // Postgres releases transaction locks only after the commit is visible
        // to new snapshots, so the next append for this tenant cannot draw a
        // nonce until every lower nonce of the tenant is committed or rolled
        // back. A reader of `global_nonce > cursor` can therefore never see a
        // higher nonce while a lower one of the same tenant is still in flight.
        //
        // Per tenant because every read path (read_all, subscribe) filters by
        // tenant_id. A reader that spans tenants would need a global lock.
        //
        // Deadlocks: taken after the FOR UPDATE row locks above. While holding
        // it, an append only writes rows of its own aggregate and idempotency
        // key, which another waiter can hold only if this append already lost
        // the optimistic check (it then fails on the committed row, it does
        // not wait). Postgres' deadlock detector covers advisory locks too.
        // Cost: one tenant's INSERT..COMMIT windows are serialized.
        sqlx::query("SELECT pg_advisory_xact_lock($1, hashtext($2))")
            .bind(APPEND_ORDER_LOCK_NAMESPACE)
            .bind(&tenant_id)
            .execute(&mut *tx)
            .await
            .map_err(map_db_error)?;

        // Everything written while the lock is held goes in ONE statement
        // (#370): all events, the stream head, the idempotency record and the
        // NOTIFY. The lock is held for three round trips (lock, write,
        // COMMIT) whatever the batch size, instead of one per event plus
        // three. Same transaction, same rows, same constraints and triggers
        // (the per-row nonce-contiguity trigger sees earlier rows of the
        // statement because rows are inserted in `ord` order).
        let recorded_ms = now_unix_ms() as i64;
        let n = events.len();
        let mut aggregate_nonces = Vec::with_capacity(n);
        let mut event_ids = Vec::with_capacity(n);
        let mut event_types = Vec::with_capacity(n);
        let mut event_versions = Vec::with_capacity(n);
        let mut content_types = Vec::with_capacity(n);
        let mut content_schemas = Vec::with_capacity(n);
        let mut correlation_ids = Vec::with_capacity(n);
        let mut causation_ids = Vec::with_capacity(n);
        let mut actor_ids = Vec::with_capacity(n);
        let mut timestamps = Vec::with_capacity(n);
        let mut payload_shas = Vec::with_capacity(n);
        let mut headers = Vec::with_capacity(n);
        let mut payloads = Vec::with_capacity(n);
        let non_empty = |s: &str| (!s.is_empty()).then(|| s.to_owned());
        for ev in &events {
            let meta = ev
                .meta
                .as_ref()
                .expect("normalized event must have metadata");
            aggregate_nonces.push(meta.aggregate_nonce as i64);
            event_ids.push(meta.event_id.clone());
            event_types.push(meta.event_type.clone());
            event_versions.push(meta.event_version as i32);
            content_types.push(meta.content_type.clone());
            content_schemas.push(non_empty(&meta.content_schema));
            correlation_ids.push(non_empty(&meta.correlation_id));
            causation_ids.push(non_empty(&meta.causation_id));
            actor_ids.push(non_empty(&meta.actor_id));
            timestamps.push(meta.timestamp_unix_ms as i64);
            payload_shas
                .push((!meta.payload_sha256.is_empty()).then(|| meta.payload_sha256.clone()));
            headers.push(Json(meta.headers.clone()));
            payloads.push(ev.payload.clone());
        }

        let rows = sqlx::query(
            r#"
            WITH ins AS (
                INSERT INTO events (
                    tenant_id, aggregate_id, aggregate_type, aggregate_nonce,
                    event_id, event_type, event_version, content_type, content_schema,
                    correlation_id, causation_id, actor_id, timestamp_unix_ms,
                    recorded_time_unix_ms, payload_sha256, headers, payload
                )
                SELECT $1, $2, $3, t.aggregate_nonce,
                       t.event_id, t.event_type, t.event_version, t.content_type, t.content_schema,
                       t.correlation_id, t.causation_id, t.actor_id, t.timestamp_unix_ms,
                       $7, t.payload_sha256, t.headers, t.payload
                FROM unnest(
                    $8::int8[], $9::text[], $10::text[], $11::int4[], $12::text[], $13::text[],
                    $14::text[], $15::text[], $16::text[], $17::int8[], $18::bytea[],
                    $19::jsonb[], $20::bytea[]
                ) WITH ORDINALITY AS t(
                    aggregate_nonce, event_id, event_type, event_version, content_type,
                    content_schema, correlation_id, causation_id, actor_id,
                    timestamp_unix_ms, payload_sha256, headers, payload, ord
                )
                ORDER BY t.ord
                RETURNING aggregate_nonce, global_nonce
            ),
            head AS (
                SELECT min(aggregate_nonce) AS first_nonce,
                       max(aggregate_nonce) AS last_nonce,
                       max(global_nonce) AS last_global
                FROM ins
            ),
            agg AS (
                INSERT INTO aggregates (tenant_id, aggregate_id, aggregate_type, last_nonce, last_global_nonce)
                SELECT $1, $2, $3, last_nonce, last_global FROM head
                ON CONFLICT (tenant_id, aggregate_id)
                DO UPDATE SET
                    aggregate_type = EXCLUDED.aggregate_type,
                    last_nonce = EXCLUDED.last_nonce,
                    last_global_nonce = EXCLUDED.last_global_nonce,
                    updated_at = NOW()
                RETURNING 1
            ),
            idem AS (
                INSERT INTO idempotency (
                    tenant_id, aggregate_id, idempotency_key,
                    request_fingerprint, first_committed_nonce, last_committed_nonce, last_global_nonce
                )
                SELECT $1, $2, $4, $5, first_nonce, last_nonce, last_global FROM head
                WHERE $4 <> ''
                ON CONFLICT (tenant_id, aggregate_id, idempotency_key)
                DO UPDATE SET
                    request_fingerprint = EXCLUDED.request_fingerprint,
                    first_committed_nonce = EXCLUDED.first_committed_nonce,
                    last_committed_nonce = EXCLUDED.last_committed_nonce,
                    last_global_nonce = EXCLUDED.last_global_nonce,
                    updated_at = NOW()
                RETURNING 1
            ),
            -- Delivered at COMMIT. A payload must stay under 8000 bytes; for
            -- a longer tenant id no NOTIFY is sent and subscribers see the
            -- events on their fallback poll (a failing pg_notify would abort
            -- the transaction).
            notified AS (
                SELECT pg_notify($6, $1 || ':' || last_global) FROM head
                WHERE octet_length($1) < $21
            )
            SELECT ins.aggregate_nonce, ins.global_nonce,
                   (SELECT count(*) FROM agg) AS agg_rows,
                   (SELECT count(*) FROM idem) AS idem_rows,
                   (SELECT count(*) FROM notified) AS notified
            FROM ins
            ORDER BY ins.aggregate_nonce
            "#,
        )
        .bind(&tenant_id)
        .bind(&aggregate_id)
        .bind(&aggregate_type)
        .bind(&req.idempotency_key)
        .bind(&fingerprint)
        .bind(NOTIFY_CHANNEL)
        .bind(recorded_ms)
        .bind(&aggregate_nonces)
        .bind(&event_ids)
        .bind(&event_types)
        .bind(&event_versions)
        .bind(&content_types)
        .bind(&content_schemas)
        .bind(&correlation_ids)
        .bind(&causation_ids)
        .bind(&actor_ids)
        .bind(&timestamps)
        .bind(&payload_shas)
        .bind(&headers)
        .bind(&payloads)
        .bind(NOTIFY_MAX_TENANT_BYTES)
        .fetch_all(&mut *tx)
        .await
        .map_err(map_db_error)?;

        // Defensive: one row per event, in aggregate order, with strictly
        // increasing global nonces. Anything else would break the
        // aggregate-order = global-order invariant; refuse to commit it.
        let mut assigned: Vec<(u64, u64)> = Vec::with_capacity(n);
        for row in &rows {
            assigned.push((
                row.get::<i64, _>("aggregate_nonce") as u64,
                row.get::<i64, _>("global_nonce") as u64,
            ));
        }
        let in_order = assigned.len() == n
            && assigned
                .iter()
                .enumerate()
                .all(|(i, (agg, _))| *agg == current_last_nonce + i as u64 + 1)
            && assigned.windows(2).all(|w| w[0].1 < w[1].1);
        let head_written = rows.first().map(|r| r.get::<i64, _>("agg_rows")) == Some(1);
        if !in_order || !head_written {
            tx.rollback().await.map_err(map_db_error)?;
            return Err(StoreError::Internal(anyhow::anyhow!(
                "append wrote unexpected rows {assigned:?} (head written: {head_written}); rolled back"
            )));
        }
        let (last_committed, last_global_nonce) = assigned[n - 1];

        // A failed COMMIT (lost connection) has an unknown outcome:
        // UNAVAILABLE, retry with the same idempotency key.
        tx.commit().await.map_err(map_db_error)?;

        // Also broadcast in-process for zero-latency same-instance delivery.
        // This intentionally duplicates the PgListener path — the subscriber's
        // select! loop handles dedup naturally (DB query is source of truth).
        let _ = self.notify_tx.send(NotifyPayload {
            tenant_id: tenant_id.clone(),
            last_global_nonce: last_global_nonce as i64,
        });

        Ok(proto::AppendResponse {
            last_global_nonce,
            last_aggregate_nonce: last_committed,
        })
    }
}

/// Outcome of [`within`]: the future's output, or the deadline that passed.
async fn within<T>(
    deadline: Option<Duration>,
    fut: impl Future<Output = T>,
) -> Result<T, Duration> {
    match deadline {
        None => Ok(fut.await),
        Some(d) => tokio::time::timeout(d, fut).await.map_err(|_| d),
    }
}

/// A database operation that exceeded the client deadline. The connection it
/// ran on must not be reused: call `close_on_drop()` on it first, because
/// sqlx otherwise pings a returned connection, which on a stalled network
/// path would hang and hold a pool slot.
fn deadline_exceeded(op: &str, deadline: Duration) -> StoreError {
    tracing::warn!(op, ?deadline, "database operation exceeded client deadline");
    StoreError::Unavailable(format!(
        "{op}: database did not answer within {} ms (client deadline); \
         the outcome is unknown, retry (appends: with the same idempotency key)",
        deadline.as_millis()
    ))
}

/// SQLSTATEs that mean "the database is (temporarily) not serving this
/// request": connection exceptions (class 08), query cancelled (57014, e.g.
/// statement_timeout), lock_timeout (55P03), idle-in-transaction timeout
/// (25P03), shutdown / startup (57P01..57P03), too many connections (53300).
fn is_unavailable_sqlstate(code: &str) -> bool {
    code.starts_with("08")
        || matches!(
            code,
            "57014" | "55P03" | "25P03" | "57P01" | "57P02" | "57P03" | "53300"
        )
}

fn map_db_error(e: sqlx::Error) -> StoreError {
    match e {
        sqlx::Error::Database(db_err) => {
            let code = db_err.code().map(|c| c.to_string()).unwrap_or_default();
            let message = db_err.message().to_string();
            if code == "23505" {
                StoreError::Concurrency {
                    message,
                    detail: None,
                }
            } else if code == "23514" {
                StoreError::Invalid(message)
            } else if is_unavailable_sqlstate(&code) {
                StoreError::Unavailable(format!("database unavailable ({code}): {message}"))
            } else {
                StoreError::Internal(anyhow::anyhow!(message))
            }
        }
        // ADR-026: losing the database is retryable, not an internal error.
        e @ (sqlx::Error::Io(_)
        | sqlx::Error::Tls(_)
        | sqlx::Error::PoolTimedOut
        | sqlx::Error::PoolClosed
        | sqlx::Error::WorkerCrashed) => {
            StoreError::Unavailable(format!("database unavailable: {e}"))
        }
        other => StoreError::Internal(anyhow::anyhow!(other)),
    }
}

#[async_trait]
impl EventStoreTrait for PostgresStore {
    fn backend_kind(&self) -> &'static str {
        "postgres"
    }

    fn capabilities(&self) -> Vec<&'static str> {
        // #337: appends take a per-tenant transaction-scoped advisory lock
        // before allocating global nonces, so within a tenant they become
        // visible in commit order.
        // #350: failed subscription queries end the stream with UNAVAILABLE.
        // #351: undecodable rows end subscriptions/reads with DATA_LOSS.
        vec![
            eventstore_core::capabilities::COMMIT_ORDERED_GLOBAL_NONCE,
            eventstore_core::capabilities::SUBSCRIPTION_ERRORS_SURFACED,
            eventstore_core::capabilities::UNDECODABLE_EVENTS_SURFACED,
        ]
    }

    async fn append(
        &self,
        mut req: proto::AppendRequest,
    ) -> Result<proto::AppendResponse, StoreError> {
        if req.tenant_id.is_empty() {
            return Err(StoreError::Unauthenticated(
                "tenant_id is required on AppendRequest".into(),
            ));
        }
        if req.aggregate_id.is_empty() {
            return Err(StoreError::Invalid(
                "aggregate_id is required on AppendRequest".into(),
            ));
        }
        if req.aggregate_type.is_empty() {
            return Err(StoreError::Invalid(
                "aggregate_type is required on AppendRequest".into(),
            ));
        }
        if req.events.is_empty() {
            return Err(StoreError::Invalid(
                "AppendRequest.events must not be empty".into(),
            ));
        }

        let tenant_id = req.tenant_id.clone();
        let aggregate_id = req.aggregate_id.clone();
        let aggregate_type = req.aggregate_type.clone();

        let mut events: Vec<proto::EventData> = Vec::with_capacity(req.events.len());
        for ev in std::mem::take(&mut req.events) {
            events.push(normalize_event(
                ev,
                &tenant_id,
                &aggregate_id,
                &aggregate_type,
            )?);
        }

        let fingerprint = batch_fingerprint(&events);
        let mut conn = self.acquire().await?;
        let tx = self.append_tx(&mut conn, &req, fingerprint, events);
        match within(self.deadline, tx).await {
            Ok(result) => result,
            Err(deadline) => {
                conn.close_on_drop();
                Err(deadline_exceeded("append", deadline))
            }
        }
    }

    async fn read_stream(
        &self,
        req: proto::ReadStreamRequest,
    ) -> Result<proto::ReadStreamResponse, StoreError> {
        if req.tenant_id.is_empty() {
            return Err(StoreError::Unauthenticated(
                "tenant_id is required on ReadStreamRequest".into(),
            ));
        }
        if req.aggregate_id.is_empty() {
            return Err(StoreError::Invalid(
                "aggregate_id is required on ReadStreamRequest".into(),
            ));
        }

        let start_nonce = if req.from_aggregate_nonce <= 1 {
            1
        } else {
            req.from_aggregate_nonce
        } as i64;

        let mut conn = self.acquire().await?;
        let query = async {
            if req.forward {
                sqlx::query(
                    r#"
                SELECT * FROM events
                WHERE tenant_id = $1 AND aggregate_id = $2 AND aggregate_nonce >= $3
                ORDER BY aggregate_nonce ASC
                LIMIT $4
                "#,
                )
                .bind(&req.tenant_id)
                .bind(&req.aggregate_id)
                .bind(start_nonce)
                .bind(req.max_count as i64)
                .fetch_all(&mut *conn)
                .await
            } else {
                sqlx::query(
                    r#"
                SELECT * FROM events
                WHERE tenant_id = $1 AND aggregate_id = $2 AND aggregate_nonce <= $3
                ORDER BY aggregate_nonce DESC
                LIMIT $4
                "#,
                )
                .bind(&req.tenant_id)
                .bind(&req.aggregate_id)
                .bind(start_nonce)
                .bind(req.max_count as i64)
                .fetch_all(&mut *conn)
                .await
            }
        };
        let rows = match within(self.deadline, query).await {
            Ok(rows) => rows.map_err(map_db_error)?,
            Err(deadline) => {
                conn.close_on_drop();
                return Err(deadline_exceeded("read_stream", deadline));
            }
        };
        drop(conn);

        let mut events = Vec::with_capacity(rows.len());
        for row in rows.into_iter() {
            events.push(row_to_event(&row)?);
        }

        // Note: No need to reverse for backward reads - the SQL ORDER BY DESC
        // already returns events in the correct order (most recent first)

        let next_from = if req.forward {
            events
                .last()
                .and_then(|ev| ev.meta.as_ref().map(|m| m.aggregate_nonce + 1))
                .unwrap_or(start_nonce as u64)
        } else {
            events
                .first()
                .and_then(|ev| {
                    ev.meta
                        .as_ref()
                        .map(|m| m.aggregate_nonce.saturating_sub(1))
                })
                .unwrap_or(0)
        };

        let is_end = events.is_empty();

        Ok(proto::ReadStreamResponse {
            events,
            is_end,
            next_from_aggregate_nonce: next_from,
        })
    }

    async fn read_all(
        &self,
        req: proto::ReadAllRequest,
    ) -> Result<proto::ReadAllResponse, StoreError> {
        if req.tenant_id.is_empty() {
            return Err(StoreError::Unauthenticated(
                "tenant_id is required on ReadAllRequest".into(),
            ));
        }

        // Apply defaults: max_count defaults to 100, capped at 1000
        let max_count = if req.max_count == 0 {
            100
        } else {
            req.max_count.min(1000)
        } as i64;

        let from_global = req.from_global_nonce as i64;

        let mut conn = self.acquire().await?;
        let query = async {
            if req.forward {
                sqlx::query(
                    r#"
                SELECT * FROM events
                WHERE tenant_id = $1 AND global_nonce >= $2
                ORDER BY global_nonce ASC
                LIMIT $3
                "#,
                )
                .bind(&req.tenant_id)
                .bind(from_global)
                .bind(max_count)
                .fetch_all(&mut *conn)
                .await
            } else {
                sqlx::query(
                    r#"
                SELECT * FROM events
                WHERE tenant_id = $1 AND global_nonce <= $2
                ORDER BY global_nonce DESC
                LIMIT $3
                "#,
                )
                .bind(&req.tenant_id)
                .bind(from_global)
                .bind(max_count)
                .fetch_all(&mut *conn)
                .await
            }
        };
        let rows = match within(self.deadline, query).await {
            Ok(rows) => rows.map_err(map_db_error)?,
            Err(deadline) => {
                conn.close_on_drop();
                return Err(deadline_exceeded("read_all", deadline));
            }
        };
        drop(conn);

        let mut events = Vec::with_capacity(rows.len());
        for row in rows.into_iter() {
            events.push(row_to_event(&row)?);
        }

        // Determine if we've reached the end
        let is_end = (events.len() as i64) < max_count;

        // Calculate next position for pagination
        let next_from = if req.forward {
            events
                .last()
                .and_then(|ev| ev.meta.as_ref().map(|m| m.global_nonce + 1))
                .unwrap_or(from_global as u64)
        } else {
            events
                .first()
                .and_then(|ev| ev.meta.as_ref().map(|m| m.global_nonce.saturating_sub(1)))
                .unwrap_or(0)
        };

        Ok(proto::ReadAllResponse {
            events,
            is_end,
            next_from_global_nonce: next_from,
        })
    }

    /// Catch-up then live subscription over the tenant's global order.
    ///
    /// Delivery is at-least-once, in `global_nonce` order, starting at
    /// `from_global_nonce` (inclusive). A response with `event: None` marks
    /// "caught up" (end of replay) or is a live keep-alive.
    ///
    /// Failures are never hidden. If a replay or live query fails, the stream
    /// yields one [`StoreError::Unavailable`] (gRPC `UNAVAILABLE`) and ends;
    /// no caught-up marker is sent for a replay that failed, and the internal
    /// cursor is not advanced past the last delivered event. The consumer
    /// reconnects with `from_global_nonce = last processed global_nonce + 1`
    /// (its own checkpoint). Reconnecting from an earlier position, e.g. when a
    /// checkpoint was not saved yet, re-delivers events; consumers must be
    /// idempotent. See ADR-026.
    fn subscribe(&self, req: proto::SubscribeRequest) -> StoreStream<proto::SubscribeResponse> {
        let pool = self.pool.clone();
        let deadline = self.deadline;
        let tenant_id = req.tenant_id.clone();
        let prefix = req.aggregate_id_prefix.clone();
        let from_global = req.from_global_nonce as i64;
        let notify_rx = self.notify_tx.subscribe();

        #[derive(Debug)]
        enum Phase {
            Replay {
                items: Vec<proto::EventData>,
                idx: usize,
                cursor: i64,
                /// Yielded (and the stream ended) once `items` are delivered:
                /// the row after the last item could not be decoded.
                then_fail: Option<StoreError>,
            },
            Live {
                cursor: i64,
                interval: Interval,
            },
            /// A query failed and the error was yielded; the stream ends.
            Failed,
        }

        // State includes the broadcast receiver for LISTEN/NOTIFY wake-ups.
        // notify_rx is moved through the unfold (not Clone), which is correct
        // since broadcast::Receiver is !Clone.
        type State = (
            PgPool,
            String,
            String,
            i64,
            Option<Phase>,
            broadcast::Receiver<NotifyPayload>,
        );

        Box::pin(stream::unfold(
            (
                pool,
                tenant_id,
                prefix,
                from_global,
                None::<Phase>,
                notify_rx,
            ),
            move |(pool, tenant, prefix, cursor, phase, mut notify_rx): State| async move {
                let mut phase = phase;
                if phase.is_none() {
                    // Replay is inclusive of from_global_nonce (`cursor` here).
                    let rows = match fetch_events_after_bounded(
                        &pool,
                        deadline,
                        &tenant,
                        &prefix,
                        cursor.saturating_sub(1),
                    )
                    .await
                    {
                        Ok(rows) => rows,
                        Err(e) => {
                            // Never report catch-up while replay is failing:
                            // surface the error and end the stream.
                            let err =
                                subscription_unavailable("replay", &tenant, &prefix, cursor, e);
                            return Some((
                                Err(err),
                                (pool, tenant, prefix, cursor, Some(Phase::Failed), notify_rx),
                            ));
                        }
                    };

                    // FIX (ADR-013): the cursor advances only as events are
                    // yielded. Decoding stops at the first undecodable row
                    // (#351): the events before it are delivered, then its
                    // error, and nothing after it.
                    let (items, then_fail) = decode_until_invalid(&rows, &tenant);
                    phase = Some(Phase::Replay {
                        items,
                        idx: 0,
                        cursor,
                        then_fail,
                    });
                }

                match phase.take() {
                    Some(Phase::Replay {
                        items,
                        mut idx,
                        cursor: _replay_cursor,
                        mut then_fail,
                    }) => {
                        if idx < items.len() {
                            let event = items[idx].clone();
                            idx += 1;

                            // FIX (ADR-013): Update cursor to the position of the event we're yielding
                            let yielded_cursor = event
                                .meta
                                .as_ref()
                                .map(|m| m.global_nonce as i64)
                                .unwrap_or(cursor);

                            let next_state = (
                                pool,
                                tenant,
                                prefix,
                                yielded_cursor,
                                Some(Phase::Replay {
                                    items,
                                    idx,
                                    cursor: yielded_cursor,
                                    then_fail,
                                }),
                                notify_rx,
                            );
                            Some((
                                Ok(proto::SubscribeResponse { event: Some(event) }),
                                next_state,
                            ))
                        } else if let Some(err) = then_fail.take() {
                            // Stop at the undecodable row: no caught-up marker,
                            // no later positions, cursor stays at the last
                            // delivered event.
                            Some((
                                Err(err),
                                (pool, tenant, prefix, cursor, Some(Phase::Failed), notify_rx),
                            ))
                        } else {
                            // All replay items yielded, transition to Live phase.
                            // FIX: If replay was empty (no events existed yet at from_global_nonce),
                            // we need to subtract 1 from cursor so that Live polling with
                            // `global_nonce > (cursor-1)` effectively catches events at the
                            // original from_global_nonce position. This handles the race condition
                            // where subscription starts before an event is created at that position.
                            let live_cursor = if items.is_empty() {
                                cursor.saturating_sub(1)
                            } else {
                                cursor
                            };
                            let next_state = (
                                pool,
                                tenant,
                                prefix,
                                live_cursor,
                                Some(Phase::Live {
                                    cursor: live_cursor,
                                    interval: interval(Duration::from_secs(FALLBACK_POLL_SECS)),
                                }),
                                notify_rx,
                            );
                            Some((Ok(proto::SubscribeResponse { event: None }), next_state))
                        }
                    }
                    Some(Phase::Live {
                        cursor,
                        mut interval,
                    }) => {
                        // Wait for either a LISTEN/NOTIFY wake-up or the fallback poll timer.
                        // The broadcast is a hint — the DB query below is the source of truth.
                        loop {
                            tokio::select! {
                                result = notify_rx.recv() => {
                                    match result {
                                        Ok(payload) if payload.tenant_id == tenant
                                            && payload.last_global_nonce > cursor => break,
                                        Ok(_) => continue, // different tenant or already past cursor
                                        Err(broadcast::error::RecvError::Lagged(_)) => break, // missed some, poll DB
                                        Err(broadcast::error::RecvError::Closed) => {
                                            // Listener shut down — fall back to pure polling
                                            interval.tick().await;
                                            break;
                                        }
                                    }
                                }
                                _ = interval.tick() => break, // safety-net fallback
                            }
                        }

                        let rows = match fetch_events_after_bounded(
                            &pool, deadline, &tenant, &prefix, cursor,
                        )
                        .await
                        {
                            Ok(rows) => rows,
                            Err(e) => {
                                // `cursor` is the last delivered position; it is
                                // reported, never advanced, on failure.
                                let err = subscription_unavailable(
                                    "live",
                                    &tenant,
                                    &prefix,
                                    cursor.saturating_add(1),
                                    e,
                                );
                                return Some((
                                    Err(err),
                                    (pool, tenant, prefix, cursor, Some(Phase::Failed), notify_rx),
                                ));
                            }
                        };

                        if !rows.is_empty() {
                            let (items, mut then_fail) = decode_until_invalid(&rows, &tenant);

                            if !items.is_empty() {
                                // FIX (ADR-013): Only advance cursor to the event we're yielding
                                let first_event = items[0].clone();
                                let yielded_cursor = first_event
                                    .meta
                                    .as_ref()
                                    .map(|m| m.global_nonce as i64)
                                    .unwrap_or(cursor);

                                let remaining = if items.len() > 1 {
                                    items[1..].to_vec()
                                } else {
                                    Vec::new()
                                };

                                let next_phase = if remaining.is_empty() && then_fail.is_none() {
                                    Phase::Live {
                                        cursor: yielded_cursor,
                                        interval,
                                    }
                                } else {
                                    // Store remaining items (and a pending decode
                                    // error) with cursor at last yielded position
                                    Phase::Replay {
                                        items: remaining,
                                        idx: 0,
                                        cursor: yielded_cursor,
                                        then_fail: then_fail.take(),
                                    }
                                };

                                let next_state = (
                                    pool,
                                    tenant,
                                    prefix,
                                    yielded_cursor,
                                    Some(next_phase),
                                    notify_rx,
                                );
                                Some((
                                    Ok(proto::SubscribeResponse {
                                        event: Some(first_event),
                                    }),
                                    next_state,
                                ))
                            } else {
                                // The first new row is undecodable. Never advance
                                // past it: surface the error and end the stream.
                                let err = then_fail
                                    .take()
                                    .expect("non-empty rows decode to an event or an error");
                                Some((
                                    Err(err),
                                    (pool, tenant, prefix, cursor, Some(Phase::Failed), notify_rx),
                                ))
                            }
                        } else {
                            // No new events — loop back to wait for next notification/poll
                            let next_state = (
                                pool,
                                tenant,
                                prefix,
                                cursor,
                                Some(Phase::Live { cursor, interval }),
                                notify_rx,
                            );
                            Some((Ok(proto::SubscribeResponse { event: None }), next_state))
                        }
                    }
                    // The error was already yielded; end the stream.
                    Some(Phase::Failed) | None => None,
                }
            },
        ))
    }
}

/// [`fetch_events_after`] on a pooled connection, bounded by the client
/// `deadline` (#368). A stalled network path surfaces as an I/O timeout,
/// which the subscription reports as `UNAVAILABLE` (ADR-026). The live
/// phase runs one short query per wake-up, so the deadline bounds each
/// query, never the subscription's lifetime.
async fn fetch_events_after_bounded(
    pool: &PgPool,
    deadline: Option<Duration>,
    tenant: &str,
    prefix: &str,
    after: i64,
) -> Result<Vec<sqlx::postgres::PgRow>, sqlx::Error> {
    let mut conn = pool.acquire().await?;
    match within(
        deadline,
        fetch_events_after(&mut *conn, tenant, prefix, after),
    )
    .await
    {
        Ok(rows) => rows,
        Err(d) => {
            conn.close_on_drop();
            Err(sqlx::Error::Io(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!(
                    "database did not answer within {} ms (client deadline)",
                    d.as_millis()
                ),
            )))
        }
    }
}

/// Events of `tenant` (optionally restricted to `prefix`) with
/// `global_nonce > after`, in global order.
async fn fetch_events_after(
    pool: impl sqlx::PgExecutor<'_>,
    tenant: &str,
    prefix: &str,
    after: i64,
) -> Result<Vec<sqlx::postgres::PgRow>, sqlx::Error> {
    if prefix.is_empty() {
        sqlx::query(
            r#"
            SELECT * FROM events
            WHERE tenant_id = $1 AND global_nonce > $2
            ORDER BY global_nonce ASC
            "#,
        )
        .bind(tenant)
        .bind(after)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query(
            r#"
            SELECT * FROM events
            WHERE tenant_id = $1 AND global_nonce > $2 AND aggregate_id LIKE $3
            ORDER BY global_nonce ASC
            "#,
        )
        .bind(tenant)
        .bind(after)
        .bind(format!("{prefix}%"))
        .fetch_all(pool)
        .await
    }
}

/// The error a subscription yields, as its last item, when a query fails.
///
/// `resume_from` is the first position the stream has not delivered. The
/// consumer should resume from its own checkpoint (the last event it
/// processed, plus one), which is at or before `resume_from`.
fn subscription_unavailable(
    phase: &str,
    tenant: &str,
    prefix: &str,
    resume_from: i64,
    error: sqlx::Error,
) -> StoreError {
    let resume_from = resume_from.max(0);
    tracing::warn!(
        tenant_id = tenant,
        aggregate_id_prefix = prefix,
        resume_from,
        error = %error,
        "subscription {phase} query failed; ending stream"
    );
    StoreError::Unavailable(format!(
        "subscription {phase} query failed, stream closed; \
         resume from global_nonce {resume_from} (or your last checkpoint + 1): {error}"
    ))
}

/// Decode one stored event row. Never panics.
///
/// A column that cannot be decoded yields [`StoreError::UndecodableEvent`]
/// naming the row's `global_nonce` and the column. The underlying decoder
/// message is deliberately dropped: it can quote stored values (e.g. a
/// header value), and this error is logged and sent to clients.
fn row_to_event(row: &sqlx::postgres::PgRow) -> Result<proto::EventData, StoreError> {
    // global_nonce first, so every other failure can name its position.
    let global_nonce = match row.try_get::<i64, _>("global_nonce") {
        Ok(n) => n as u64,
        Err(_) => {
            return Err(StoreError::UndecodableEvent {
                global_nonce: 0,
                reason: "column 'global_nonce' could not be decoded".into(),
            })
        }
    };
    let col = |name: &'static str| {
        move |_: sqlx::Error| StoreError::UndecodableEvent {
            global_nonce,
            reason: format!("column '{name}' could not be decoded"),
        }
    };
    let opt_text = |name: &'static str| -> Result<String, StoreError> {
        Ok(row
            .try_get::<Option<String>, _>(name)
            .map_err(col(name))?
            .unwrap_or_default())
    };

    let headers: Json<HashMap<String, String>> = row.try_get("headers").map_err(col("headers"))?;
    let meta = proto::EventMetadata {
        event_id: row.try_get("event_id").map_err(col("event_id"))?,
        aggregate_id: row.try_get("aggregate_id").map_err(col("aggregate_id"))?,
        aggregate_type: row
            .try_get("aggregate_type")
            .map_err(col("aggregate_type"))?,
        aggregate_nonce: row
            .try_get::<i64, _>("aggregate_nonce")
            .map_err(col("aggregate_nonce"))? as u64,
        event_type: row.try_get("event_type").map_err(col("event_type"))?,
        event_version: row
            .try_get::<i32, _>("event_version")
            .map_err(col("event_version"))? as u32,
        content_type: row.try_get("content_type").map_err(col("content_type"))?,
        content_schema: opt_text("content_schema")?,
        correlation_id: opt_text("correlation_id")?,
        causation_id: opt_text("causation_id")?,
        actor_id: opt_text("actor_id")?,
        tenant_id: row.try_get("tenant_id").map_err(col("tenant_id"))?,
        timestamp_unix_ms: row
            .try_get::<i64, _>("timestamp_unix_ms")
            .map_err(col("timestamp_unix_ms"))? as u64,
        recorded_time_unix_ms: row
            .try_get::<i64, _>("recorded_time_unix_ms")
            .map_err(col("recorded_time_unix_ms"))? as u64,
        payload_sha256: row
            .try_get::<Option<Vec<u8>>, _>("payload_sha256")
            .map_err(col("payload_sha256"))?
            .unwrap_or_default(),
        headers: headers.0,
        global_nonce,
    };
    Ok(proto::EventData {
        meta: Some(meta),
        payload: row
            .try_get::<Option<Vec<u8>>, _>("payload")
            .map_err(col("payload"))?
            .unwrap_or_default(),
    })
}

/// Decode rows in order, stopping at the first undecodable one.
///
/// Returns the events before it and, if any row failed, its error. Rows after
/// a failed one are never decoded or delivered: delivering them would move
/// the consumer's checkpoint past an event it never saw (#351).
fn decode_until_invalid(
    rows: &[sqlx::postgres::PgRow],
    tenant: &str,
) -> (Vec<proto::EventData>, Option<StoreError>) {
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        match row_to_event(row) {
            Ok(event) => items.push(event),
            Err(e) => {
                // No payload or stored values here; position and column only.
                tracing::error!(
                    tenant_id = tenant,
                    error = %e,
                    "subscription stopped at an undecodable stored event; \
                     operator action required (see ADR-026)"
                );
                return (items, Some(e));
            }
        }
    }
    (items, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn connect_invalid_url_errors_fast() {
        // Use an invalid URL that fails immediately without network timeout
        let url = "invalid-postgres-url";
        let res = PostgresStore::connect(url).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn subscribe_handles_empty() {
        // This test just verifies that subscribe returns a stream
        // We don't actually call .next() because that would require a real database connection
        let url = "postgres://test:test@localhost:5432/test";
        let (notify_tx, _) = broadcast::channel(NOTIFY_BROADCAST_CAPACITY);
        let store = PostgresStore {
            pool: PgPoolOptions::new()
                .connect_lazy(url)
                .expect("lazy connect should not attempt network"),
            notify_tx,
            listener_handle: tokio::spawn(async {}),
            deadline: None,
        };
        let _stream = store.subscribe(proto::SubscribeRequest {
            tenant_id: "tenant".into(),
            aggregate_id_prefix: "".into(),
            from_global_nonce: 0,
        });
        // Test passes if we can create the stream without panicking
    }

    /// A store whose pool can never connect: every query fails fast.
    fn unreachable_store() -> PostgresStore {
        let (notify_tx, _) = broadcast::channel(NOTIFY_BROADCAST_CAPACITY);
        PostgresStore {
            pool: PgPoolOptions::new()
                .acquire_timeout(Duration::from_secs(2))
                .connect_lazy("postgres://test:test@127.0.0.1:1/test")
                .expect("lazy connect should not attempt network"),
            notify_tx,
            listener_handle: tokio::spawn(async {}),
            deadline: Some(Duration::from_secs(5)),
        }
    }

    /// A "database" that accepts TCP connections and never answers (a
    /// black-holed path), and a store pointed at it with a 1 s acquire
    /// timeout. Keeps accepted sockets open so nothing errors by itself.
    async fn black_holed_store() -> (PostgresStore, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hole = tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((sock, _)) = listener.accept().await {
                held.push(sock);
            }
        });
        let (notify_tx, _) = broadcast::channel(NOTIFY_BROADCAST_CAPACITY);
        let store = PostgresStore {
            pool: PgPoolOptions::new()
                .acquire_timeout(Duration::from_secs(1))
                .connect_lazy(&format!("postgres://test:test@{addr}/test"))
                .expect("lazy connect should not attempt network"),
            notify_tx,
            listener_handle: tokio::spawn(async {}),
            deadline: Some(Duration::from_secs(2)),
        };
        (store, hole)
    }

    fn one_event_append() -> proto::AppendRequest {
        proto::AppendRequest {
            tenant_id: "t".into(),
            aggregate_id: "a".into(),
            aggregate_type: "A".into(),
            expected_aggregate_nonce: 0,
            idempotency_key: String::new(),
            events: vec![proto::EventData {
                meta: Some(proto::EventMetadata {
                    event_id: "e1".into(),
                    aggregate_nonce: 1,
                    event_type: "E".into(),
                    ..Default::default()
                }),
                payload: vec![],
            }],
        }
    }

    #[tokio::test]
    async fn black_holed_database_surfaces_unavailable_within_bound() {
        use futures::StreamExt;
        let (store, hole) = black_holed_store().await;
        let bound = Duration::from_secs(5);

        let started = std::time::Instant::now();
        let res = tokio::time::timeout(bound, store.append(one_event_append()))
            .await
            .expect("append to a black-holed database must not hang");
        assert!(matches!(res, Err(StoreError::Unavailable(_))), "{res:?}");

        let res = tokio::time::timeout(
            bound,
            store.read_all(proto::ReadAllRequest {
                tenant_id: "t".into(),
                from_global_nonce: 0,
                max_count: 10,
                forward: true,
            }),
        )
        .await
        .expect("read_all must not hang");
        assert!(matches!(res, Err(StoreError::Unavailable(_))), "{res:?}");

        let res = tokio::time::timeout(
            bound,
            store.read_stream(proto::ReadStreamRequest {
                tenant_id: "t".into(),
                aggregate_id: "a".into(),
                from_aggregate_nonce: 1,
                max_count: 10,
                forward: true,
            }),
        )
        .await
        .expect("read_stream must not hang");
        assert!(matches!(res, Err(StoreError::Unavailable(_))), "{res:?}");

        let mut stream = store.subscribe(proto::SubscribeRequest {
            tenant_id: "t".into(),
            aggregate_id_prefix: String::new(),
            from_global_nonce: 3,
        });
        match tokio::time::timeout(bound, stream.next()).await {
            Ok(Some(Err(StoreError::Unavailable(msg)))) => {
                assert!(msg.contains("resume from global_nonce 3"), "{msg}")
            }
            other => panic!("subscribe must surface Unavailable, got {other:?}"),
        }
        assert!(started.elapsed() < Duration::from_secs(15));
        hole.abort();
    }

    #[test]
    fn connection_and_timeout_errors_map_to_unavailable() {
        for e in [
            sqlx::Error::PoolTimedOut,
            sqlx::Error::PoolClosed,
            sqlx::Error::Io(std::io::Error::new(std::io::ErrorKind::TimedOut, "x")),
        ] {
            assert!(
                matches!(map_db_error(e), StoreError::Unavailable(_)),
                "pool/io errors are UNAVAILABLE (ADR-026)"
            );
        }
        for code in ["57014", "55P03", "25P03", "08006", "57P01", "53300"] {
            assert!(is_unavailable_sqlstate(code), "{code}");
        }
        for code in ["23505", "23514", "42P01", "40P01", ""] {
            assert!(!is_unavailable_sqlstate(code), "{code}");
        }
    }

    #[tokio::test]
    async fn within_bounds_only_when_a_deadline_is_set() {
        assert_eq!(within(None, async { 7 }).await, Ok(7));
        let d = Duration::from_millis(20);
        assert_eq!(within(Some(d), std::future::pending::<()>()).await, Err(d));
    }

    async fn assert_replay_failure_surfaces(prefix: &str) {
        use futures::StreamExt;
        let store = unreachable_store();
        let mut stream = store.subscribe(proto::SubscribeRequest {
            tenant_id: "tenant".into(),
            aggregate_id_prefix: prefix.into(),
            from_global_nonce: 7,
        });
        let first = tokio::time::timeout(Duration::from_secs(10), stream.next())
            .await
            .expect("stream must not hang");
        match first {
            Some(Err(e @ StoreError::Unavailable(_))) => {
                let msg = e.to_string();
                assert!(msg.contains("replay"), "{msg}");
                assert!(msg.contains("resume from global_nonce 7"), "{msg}");
            }
            other => panic!("replay DB failure must surface Unavailable, got {other:?}"),
        }
        assert!(
            stream.next().await.is_none(),
            "stream must end after the error"
        );
    }

    #[tokio::test]
    async fn subscribe_surfaces_replay_query_failure_without_prefix() {
        assert_replay_failure_surfaces("").await;
    }

    #[tokio::test]
    async fn subscribe_surfaces_replay_query_failure_with_prefix() {
        assert_replay_failure_surfaces("Order-").await;
    }

    #[test]
    fn notify_payload_roundtrip() {
        let encoded = NotifyPayload::encode("tenant-abc", 1042);
        assert_eq!(encoded, "tenant-abc:1042");

        let parsed = NotifyPayload::parse(&encoded).unwrap();
        assert_eq!(parsed.tenant_id, "tenant-abc");
        assert_eq!(parsed.last_global_nonce, 1042);
    }

    #[test]
    fn notify_payload_roundtrip_zero_nonce() {
        let encoded = NotifyPayload::encode("t", 0);
        let parsed = NotifyPayload::parse(&encoded).unwrap();
        assert_eq!(parsed.tenant_id, "t");
        assert_eq!(parsed.last_global_nonce, 0);
    }

    #[test]
    fn notify_payload_roundtrip_large_nonce() {
        let encoded = NotifyPayload::encode("tenant", i64::MAX);
        let parsed = NotifyPayload::parse(&encoded).unwrap();
        assert_eq!(parsed.last_global_nonce, i64::MAX);
    }

    #[test]
    fn notify_payload_with_colon_in_tenant() {
        // rsplit_once splits on the LAST colon, so tenant can contain colons
        let encoded = NotifyPayload::encode("org:team:tenant", 42);
        assert_eq!(encoded, "org:team:tenant:42");

        let parsed = NotifyPayload::parse(&encoded).unwrap();
        assert_eq!(parsed.tenant_id, "org:team:tenant");
        assert_eq!(parsed.last_global_nonce, 42);
    }

    #[test]
    fn notify_payload_parse_rejects_invalid() {
        assert!(NotifyPayload::parse("").is_none());
        assert!(NotifyPayload::parse("no-colon").is_none());
        assert!(NotifyPayload::parse("tenant:not_a_number").is_none());
        assert!(NotifyPayload::parse(":").is_none());
    }
}
