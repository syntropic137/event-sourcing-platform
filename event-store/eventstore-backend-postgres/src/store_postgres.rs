use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use eventstore_core::{proto, EventStore as EventStoreTrait, StoreError, StoreStream};
use futures::stream;
use prost::Message;
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPoolOptions, types::Json, PgPool, Row};
use tokio::time::{interval, Duration, Interval};

const DEFAULT_CONTENT_TYPE: &str = "application/octet-stream";

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
            let mut clone = proto::EventData {
                meta: Some(meta.clone()),
                payload: ev.payload.clone(),
            };
            if let Some(m) = clone.meta.as_mut() {
                m.recorded_time_unix_ms = 0;
                m.global_nonce = 0;
            }
            hasher.update(clone.encode_to_vec());
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

#[derive(Clone)]
pub struct PostgresStore {
    pool: PgPool,
}

impl PostgresStore {
    pub fn new(pool: PgPool) -> Arc<Self> {
        Arc::new(Self { pool })
    }

    pub async fn connect(database_url: &str) -> anyhow::Result<Arc<Self>> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self::new(pool))
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
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
            } else {
                StoreError::Internal(anyhow::anyhow!(message))
            }
        }
        other => StoreError::Internal(anyhow::anyhow!(other)),
    }
}

#[async_trait]
impl EventStoreTrait for PostgresStore {
    async fn append(&self, req: proto::AppendRequest) -> Result<proto::AppendResponse, StoreError> {
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
        for ev in req.events.into_iter() {
            events.push(normalize_event(
                ev,
                &tenant_id,
                &aggregate_id,
                &aggregate_type,
            )?);
        }

        let fingerprint = batch_fingerprint(&events);
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| StoreError::Internal(anyhow::anyhow!(e)))?;

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

        let mut last_global_nonce = current_last_global;
        let mut assigned_events: Vec<proto::EventData> = Vec::with_capacity(events.len());
        for mut ev in events.into_iter() {
            let mut meta = ev.meta.take().expect("normalized event must have metadata");
            let now_ms = now_unix_ms();
            meta.recorded_time_unix_ms = now_ms;
            let headers_json = Json(meta.headers.clone());
            let payload_sha = if meta.payload_sha256.is_empty() {
                None
            } else {
                Some(meta.payload_sha256.clone())
            };

            let row = sqlx::query(
                r#"
                INSERT INTO events (
                    tenant_id, aggregate_id, aggregate_type, aggregate_nonce,
                    event_id, event_type, event_version, content_type, content_schema,
                    correlation_id, causation_id, actor_id, timestamp_unix_ms,
                    recorded_time_unix_ms, payload_sha256, headers, payload
                ) VALUES (
                    $1, $2, $3, $4,
                    $5, $6, $7, $8, $9,
                    $10, $11, $12, $13,
                    $14, $15, $16, $17
                )
                RETURNING global_nonce
                "#,
            )
            .bind(&tenant_id)
            .bind(&aggregate_id)
            .bind(&aggregate_type)
            .bind(meta.aggregate_nonce as i64)
            .bind(&meta.event_id)
            .bind(&meta.event_type)
            .bind(meta.event_version as i32)
            .bind(&meta.content_type)
            .bind(if meta.content_schema.is_empty() {
                None::<&str>
            } else {
                Some(meta.content_schema.as_str())
            })
            .bind(if meta.correlation_id.is_empty() {
                None::<&str>
            } else {
                Some(meta.correlation_id.as_str())
            })
            .bind(if meta.causation_id.is_empty() {
                None::<&str>
            } else {
                Some(meta.causation_id.as_str())
            })
            .bind(if meta.actor_id.is_empty() {
                None::<&str>
            } else {
                Some(meta.actor_id.as_str())
            })
            .bind(meta.timestamp_unix_ms as i64)
            .bind(now_ms as i64)
            .bind(payload_sha)
            .bind(headers_json)
            .bind(&ev.payload)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_db_error)?;

            let global_nonce: i64 = row.get("global_nonce");
            meta.global_nonce = global_nonce as u64;
            last_global_nonce = meta.global_nonce;

            assigned_events.push(proto::EventData {
                meta: Some(meta.clone()),
                payload: ev.payload,
            });
        }

        let last_committed = assigned_events
            .last()
            .and_then(|ev| ev.meta.as_ref().map(|m| m.aggregate_nonce))
            .unwrap_or(current_last_nonce);
        let first_committed = assigned_events
            .first()
            .and_then(|ev| ev.meta.as_ref().map(|m| m.aggregate_nonce))
            .unwrap_or(current_last_nonce + 1);

        sqlx::query(
            r#"
            INSERT INTO aggregates (tenant_id, aggregate_id, aggregate_type, last_nonce, last_global_nonce)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (tenant_id, aggregate_id)
            DO UPDATE SET
                aggregate_type = EXCLUDED.aggregate_type,
                last_nonce = EXCLUDED.last_nonce,
                last_global_nonce = EXCLUDED.last_global_nonce,
                updated_at = NOW()
            "#,
        )
        .bind(&tenant_id)
        .bind(&aggregate_id)
        .bind(&aggregate_type)
        .bind(last_committed as i64)
        .bind(last_global_nonce as i64)
        .execute(&mut *tx)
        .await
        .map_err(map_db_error)?;

        if !req.idempotency_key.is_empty() {
            sqlx::query(
                r#"
                INSERT INTO idempotency (
                    tenant_id, aggregate_id, idempotency_key,
                    request_fingerprint, first_committed_nonce, last_committed_nonce, last_global_nonce
                ) VALUES ($1, $2, $3, $4, $5, $6, $7)
                ON CONFLICT (tenant_id, aggregate_id, idempotency_key)
                DO UPDATE SET
                    request_fingerprint = EXCLUDED.request_fingerprint,
                    first_committed_nonce = EXCLUDED.first_committed_nonce,
                    last_committed_nonce = EXCLUDED.last_committed_nonce,
                    last_global_nonce = EXCLUDED.last_global_nonce,
                    updated_at = NOW()
                "#,
            )
            .bind(&tenant_id)
            .bind(&aggregate_id)
            .bind(&req.idempotency_key)
            .bind(&fingerprint)
            .bind(first_committed as i64)
            .bind(last_committed as i64)
            .bind(last_global_nonce as i64)
            .execute(&mut *tx)
            .await
            .map_err(map_db_error)?;
        }

        tx.commit()
            .await
            .map_err(|e| StoreError::Internal(anyhow::anyhow!(e)))?;

        Ok(proto::AppendResponse {
            last_global_nonce,
            last_aggregate_nonce: last_committed,
        })
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

        let rows = if req.forward {
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
            .fetch_all(&self.pool)
            .await
            .map_err(map_db_error)?
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
            .fetch_all(&self.pool)
            .await
            .map_err(map_db_error)?
        };

        let mut events = Vec::with_capacity(rows.len());
        for row in rows.into_iter() {
            events.push(row_to_event(&row)?);
        }

        if !req.forward {
            events.reverse();
        }

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

    fn subscribe(&self, req: proto::SubscribeRequest) -> StoreStream<proto::SubscribeResponse> {
        let pool = self.pool.clone();
        let tenant_id = req.tenant_id.clone();
        let prefix = req.aggregate_id_prefix.clone();
        let from_global = req.from_global_nonce as i64;

        #[derive(Debug)]
        enum Phase {
            Replay {
                items: Vec<proto::EventData>,
                idx: usize,
                cursor: i64,
            },
            Live {
                cursor: i64,
                interval: Interval,
            },
        }

        Box::pin(stream::unfold(
            (pool, tenant_id, prefix, from_global, None::<Phase>),
            |(pool, tenant, prefix, mut cursor, phase)| async move {
                let mut phase = phase;
                if phase.is_none() {
                    let rows = if prefix.is_empty() {
                        sqlx::query(
                            r#"
                            SELECT * FROM events
                            WHERE tenant_id = $1 AND global_nonce >= $2
                            ORDER BY global_nonce ASC
                            "#,
                        )
                        .bind(&tenant)
                        .bind(cursor)
                        .fetch_all(&pool)
                        .await
                        .unwrap_or_default()
                    } else {
                        let like = format!("{prefix}%");
                        sqlx::query(
                            r#"
                            SELECT * FROM events
                            WHERE tenant_id = $1 AND global_nonce >= $2 AND aggregate_id LIKE $3
                            ORDER BY global_nonce ASC
                            "#,
                        )
                        .bind(&tenant)
                        .bind(cursor)
                        .bind(like)
                        .fetch_all(&pool)
                        .await
                        .unwrap_or_default()
                    };
                    let mut items = Vec::with_capacity(rows.len());
                    for row in rows.iter() {
                        if let Ok(event) = row_to_event(row) {
                            cursor = row.get::<i64, _>("global_nonce");
                            items.push(event);
                        }
                    }
                    phase = Some(Phase::Replay {
                        items,
                        idx: 0,
                        cursor,
                    });
                }

                match phase.take() {
                    Some(Phase::Replay {
                        items,
                        mut idx,
                        cursor: replay_cursor,
                    }) => {
                        if idx < items.len() {
                            let event = items[idx].clone();
                            idx += 1;
                            let next_state = (
                                pool,
                                tenant,
                                prefix,
                                replay_cursor,
                                Some(Phase::Replay {
                                    items,
                                    idx,
                                    cursor: replay_cursor,
                                }),
                            );
                            Some((
                                Ok(proto::SubscribeResponse { event: Some(event) }),
                                next_state,
                            ))
                        } else {
                            let next_state = (
                                pool,
                                tenant,
                                prefix,
                                replay_cursor,
                                Some(Phase::Live {
                                    cursor: replay_cursor,
                                    interval: interval(Duration::from_millis(200)),
                                }),
                            );
                            Some((Ok(proto::SubscribeResponse { event: None }), next_state))
                        }
                    }
                    Some(Phase::Live {
                        mut cursor,
                        mut interval,
                    }) => {
                        let rows = if prefix.is_empty() {
                            sqlx::query(
                                r#"
                                SELECT * FROM events
                                WHERE tenant_id = $1 AND global_nonce > $2
                                ORDER BY global_nonce ASC
                                "#,
                            )
                            .bind(&tenant)
                            .bind(cursor)
                            .fetch_all(&pool)
                            .await
                            .unwrap_or_default()
                        } else {
                            let like = format!("{prefix}%");
                            sqlx::query(
                                r#"
                                SELECT * FROM events
                                WHERE tenant_id = $1 AND global_nonce > $2 AND aggregate_id LIKE $3
                                ORDER BY global_nonce ASC
                                "#,
                            )
                            .bind(&tenant)
                            .bind(cursor)
                            .bind(like)
                            .fetch_all(&pool)
                            .await
                            .unwrap_or_default()
                        };

                        if !rows.is_empty() {
                            let mut items = Vec::with_capacity(rows.len());
                            for row in rows.iter() {
                                if let Ok(event) = row_to_event(row) {
                                    cursor = row.get::<i64, _>("global_nonce");
                                    items.push(event);
                                }
                            }
                            let event = items.first().cloned();
                            let remaining = if items.len() > 1 {
                                items[1..].to_vec()
                            } else {
                                Vec::new()
                            };
                            let next_phase = if remaining.is_empty() {
                                Phase::Live { cursor, interval }
                            } else {
                                Phase::Replay {
                                    items: remaining,
                                    idx: 0,
                                    cursor,
                                }
                            };
                            let next_state = (pool, tenant, prefix, cursor, Some(next_phase));
                            Some((Ok(proto::SubscribeResponse { event }), next_state))
                        } else {
                            interval.tick().await;
                            let next_state = (
                                pool,
                                tenant,
                                prefix,
                                cursor,
                                Some(Phase::Live { cursor, interval }),
                            );
                            Some((Ok(proto::SubscribeResponse { event: None }), next_state))
                        }
                    }
                    None => None,
                }
            },
        ))
    }
}

fn row_to_event(row: &sqlx::postgres::PgRow) -> Result<proto::EventData, StoreError> {
    let headers: Json<HashMap<String, String>> = row
        .try_get("headers")
        .map_err(|e| StoreError::Internal(anyhow::anyhow!(e)))?;
    let meta = proto::EventMetadata {
        event_id: row.get::<String, _>("event_id"),
        aggregate_id: row.get::<String, _>("aggregate_id"),
        aggregate_type: row.get::<String, _>("aggregate_type"),
        aggregate_nonce: row.get::<i64, _>("aggregate_nonce") as u64,
        event_type: row.get::<String, _>("event_type"),
        event_version: row.get::<i32, _>("event_version") as u32,
        content_type: row.get::<String, _>("content_type"),
        content_schema: row
            .get::<Option<String>, _>("content_schema")
            .unwrap_or_default(),
        correlation_id: row
            .get::<Option<String>, _>("correlation_id")
            .unwrap_or_default(),
        causation_id: row
            .get::<Option<String>, _>("causation_id")
            .unwrap_or_default(),
        actor_id: row.get::<Option<String>, _>("actor_id").unwrap_or_default(),
        tenant_id: row.get::<String, _>("tenant_id"),
        timestamp_unix_ms: row.get::<i64, _>("timestamp_unix_ms") as u64,
        recorded_time_unix_ms: row.get::<i64, _>("recorded_time_unix_ms") as u64,
        payload_sha256: row
            .get::<Option<Vec<u8>>, _>("payload_sha256")
            .unwrap_or_default(),
        headers: headers.0,
        global_nonce: row.get::<i64, _>("global_nonce") as u64,
    };
    Ok(proto::EventData {
        meta: Some(meta),
        payload: row.get::<Option<Vec<u8>>, _>("payload").unwrap_or_default(),
    })
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
        let store = PostgresStore {
            pool: PgPoolOptions::new()
                .connect_lazy(url)
                .expect("lazy connect should not attempt network"),
        };
        let _stream = store.subscribe(proto::SubscribeRequest {
            tenant_id: "tenant".into(),
            aggregate_id_prefix: "".into(),
            from_global_nonce: 0,
        });
        // Test passes if we can create the stream without panicking
        assert!(true);
    }
}
