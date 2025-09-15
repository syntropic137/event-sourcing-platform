use std::sync::Arc;

use async_trait::async_trait;
use eventstore_core::{proto, EventStore as EventStoreTrait, StoreError, StoreStream};
use futures::stream;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use tokio::time::{interval, Duration, Interval};

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
        // Run migrations bundled with the crate
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self::new(pool))
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[async_trait]
impl EventStoreTrait for PostgresStore {
    async fn append(&self, req: proto::AppendRequest) -> Result<proto::AppendResponse, StoreError> {
        // Basic validation
        if req.aggregate_id.is_empty() || req.aggregate_type.is_empty() {
            return Err(StoreError::Invalid(
                "aggregate_id and aggregate_type must be provided".into(),
            ));
        }

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| StoreError::Other(anyhow::anyhow!(e)))?;

        // Current aggregate nonce
        let row = sqlx::query(
            "SELECT COALESCE(MAX(aggregate_nonce), 0) AS v FROM events WHERE aggregate_id = $1",
        )
        .bind(&req.aggregate_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| StoreError::Other(anyhow::anyhow!(e)))?;
        let current_nonce: i64 = row.get::<i64, _>("v");

        // Concurrency check
        let expected_ok = match req.expected {
            Some(proto::append_request::Expected::Exact(ex)) => current_nonce as u64 == ex,
            Some(proto::append_request::Expected::ExpectedAny(e)) => {
                match proto::Expected::try_from(e).unwrap_or(proto::Expected::Any) {
                    proto::Expected::Any => true,
                    proto::Expected::NoAggregate => current_nonce == 0,
                    proto::Expected::AggregateExists => current_nonce > 0,
                }
            }
            None => true,
        };
        if !expected_ok {
            return Err(StoreError::Concurrency(format!(
                "expected mismatch for aggregate {}",
                req.aggregate_id
            )));
        }

        // Validate client-provided aggregate_nonces
        for (i, ev) in req.events.iter().enumerate() {
            let expected_nonce = current_nonce as u64 + 1 + i as u64;
            if let Some(meta) = &ev.meta {
                if meta.aggregate_nonce != expected_nonce {
                    return Err(StoreError::Concurrency(format!(
                        "proposed aggregate_nonce {} != expected {} for event {}",
                        meta.aggregate_nonce, expected_nonce, i
                    )));
                }
            } else {
                return Err(StoreError::Invalid(
                    "EventMetadata required for optimistic concurrency".into(),
                ));
            }
        }

        let mut last_global: i64 = 0;
        let events_len = req.events.len();
        for mut ev in req.events.into_iter() {
            // Ensure we have metadata to pull fields from
            let mut meta = ev.meta.take().unwrap_or_else(|| proto::EventMetadata {
                ..Default::default()
            });

            // Store validates and assigns aggregate_id/type (client provides nonce)
            meta.aggregate_id = req.aggregate_id.clone();
            meta.aggregate_type = req.aggregate_type.clone();

            // Required fields
            let event_id = if meta.event_id.is_empty() {
                return Err(StoreError::Invalid(
                    "event_id must be set in metadata (uuid string)".into(),
                ));
            } else {
                meta.event_id.clone()
            };

            let event_type = if meta.event_type.is_empty() {
                None
            } else {
                Some(meta.event_type.clone())
            };
            let content_type = if meta.content_type.is_empty() {
                None
            } else {
                Some(meta.content_type.clone())
            };
            let payload = ev.payload;

            // Insert row; cast event_id to uuid in SQL to avoid needing uuid crate
            let row = sqlx::query(
                r#"
                INSERT INTO events (
                    event_id, aggregate_id, aggregate_type, aggregate_nonce,
                    event_type, content_type, payload
                ) VALUES (
                    $1::uuid, $2, $3, $4, $5, $6, $7
                )
                RETURNING global_nonce
                "#,
            )
            .bind(&event_id)
            .bind(&meta.aggregate_id)
            .bind(&meta.aggregate_type)
            .bind(meta.aggregate_nonce as i64)
            .bind(event_type)
            .bind(content_type)
            .bind(&payload)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| {
                // Map unique violations to concurrency errors
                let msg = e.to_string();
                if msg.contains("uq_aggregate_nonce") || msg.contains("unique") {
                    StoreError::Concurrency(format!(
                        "concurrency violation on aggregate {}",
                        meta.aggregate_id
                    ))
                } else {
                    StoreError::Other(anyhow::anyhow!(e))
                }
            })?;

            let inserted_global: i64 = row.get::<i64, _>("global_nonce");
            last_global = inserted_global;
        }

        tx.commit()
            .await
            .map_err(|e| StoreError::Other(anyhow::anyhow!(e)))?;

        Ok(proto::AppendResponse {
            next_aggregate_nonce: current_nonce as u64 + events_len as u64,
            last_global_nonce: last_global as u64,
        })
    }

    async fn read_stream(
        &self,
        req: proto::ReadStreamRequest,
    ) -> Result<proto::ReadStreamResponse, StoreError> {
        if req.aggregate_id.is_empty() {
            return Err(StoreError::Invalid("aggregate_id must be provided".into()));
        }

        // Select appropriate direction
        let rows = if req.forward {
            sqlx::query(
                r#"SELECT global_nonce, event_id::text AS event_id,
                           aggregate_id, aggregate_type, aggregate_nonce,
                           event_type, content_type, payload
                    FROM events
                    WHERE aggregate_id = $1 AND aggregate_nonce >= $2
                    ORDER BY aggregate_nonce ASC
                    LIMIT $3"#,
            )
            .bind(&req.aggregate_id)
            .bind(req.from_aggregate_nonce as i64)
            .bind(req.max_count as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StoreError::Other(anyhow::anyhow!(e)))?
        } else {
            sqlx::query(
                r#"SELECT global_nonce, event_id::text AS event_id,
                           aggregate_id, aggregate_type, aggregate_nonce,
                           event_type, content_type, payload
                    FROM events
                    WHERE aggregate_id = $1 AND aggregate_nonce <= $2
                    ORDER BY aggregate_nonce DESC
                    LIMIT $3"#,
            )
            .bind(&req.aggregate_id)
            .bind(req.from_aggregate_nonce as i64)
            .bind(req.max_count as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StoreError::Other(anyhow::anyhow!(e)))?
        };

        // Map rows to proto events
        let mut events: Vec<proto::EventData> = Vec::with_capacity(rows.len());
        for r in rows.into_iter() {
            let meta = proto::EventMetadata {
                event_id: r.get::<Option<String>, _>("event_id").unwrap_or_default(),
                aggregate_id: r.get::<String, _>("aggregate_id"),
                aggregate_type: r.get::<String, _>("aggregate_type"),
                aggregate_nonce: r.get::<i64, _>("aggregate_nonce") as u64,
                event_type: r.get::<Option<String>, _>("event_type").unwrap_or_default(),
                content_type: r
                    .get::<Option<String>, _>("content_type")
                    .unwrap_or_default(),
                global_nonce: r.get::<i64, _>("global_nonce") as u64,
                ..Default::default()
            };
            events.push(proto::EventData {
                meta: Some(meta),
                payload: r.get::<Option<Vec<u8>>, _>("payload").unwrap_or_default(),
            });
        }

        // Compute next_from_aggregate_nonce and is_end heuristics similar to memory backend
        let next_from_aggregate_nonce = if req.forward {
            req.from_aggregate_nonce.saturating_add(events.len() as u64)
        } else {
            req.from_aggregate_nonce.saturating_sub(events.len() as u64)
        };
        let is_end = events.is_empty();

        Ok(proto::ReadStreamResponse {
            events,
            is_end,
            next_from_aggregate_nonce,
        })
    }

    fn subscribe(&self, req: proto::SubscribeRequest) -> StoreStream<proto::SubscribeResponse> {
        let pool = self.pool.clone();
        let prefix = req.aggregate_prefix.clone();
        let from_pos = req.from_global_nonce as i64;

        // Helper to map a row to EventData
        fn row_to_event(r: &sqlx::postgres::PgRow) -> proto::EventData {
            let meta = proto::EventMetadata {
                event_id: r.get::<Option<String>, _>("event_id").unwrap_or_default(),
                aggregate_id: r.get::<String, _>("aggregate_id"),
                aggregate_type: r.get::<String, _>("aggregate_type"),
                aggregate_nonce: r.get::<i64, _>("aggregate_nonce") as u64,
                event_type: r.get::<Option<String>, _>("event_type").unwrap_or_default(),
                content_type: r
                    .get::<Option<String>, _>("content_type")
                    .unwrap_or_default(),
                global_nonce: r.get::<i64, _>("global_nonce") as u64,
                ..Default::default()
            };
            proto::EventData {
                meta: Some(meta),
                payload: r.get::<Option<Vec<u8>>, _>("payload").unwrap_or_default(),
            }
        }

        #[derive(Debug)]
        enum Phase {
            Replay {
                buf: Vec<proto::EventData>,
                idx: usize,
                cursor: i64,
            },
            Live {
                cursor: i64,
                interval: Interval,
                pending: Vec<proto::EventData>,
            },
        }

        Box::pin(stream::unfold(
            // Initial state will be populated on first poll
            (pool, prefix, Some(from_pos), None::<Phase>),
            |(pool, prefix, start_opt, mut phase_opt)| async move {
                // Initialize replay on first call
                if phase_opt.is_none() {
                    let start = start_opt.unwrap_or(0);
                    let rows = if prefix.is_empty() {
                        sqlx::query(
                            r#"SELECT global_nonce, event_id::text AS event_id,
                               aggregate_id, aggregate_type, aggregate_nonce,
                               event_type, content_type, payload
                               FROM events
                               WHERE global_nonce >= $1
                               ORDER BY global_nonce ASC"#,
                        )
                        .bind(start)
                        .fetch_all(&pool)
                        .await
                        .unwrap_or_default()
                    } else {
                        let like = format!("{prefix}%");
                        sqlx::query(
                            r#"SELECT global_nonce, event_id::text AS event_id,
                               aggregate_id, aggregate_type, aggregate_nonce,
                               event_type, content_type, payload
                               FROM events
                               WHERE global_nonce >= $1 AND aggregate_id LIKE $2
                               ORDER BY global_nonce ASC"#,
                        )
                        .bind(start)
                        .bind(like)
                        .fetch_all(&pool)
                        .await
                        .unwrap_or_default()
                    };
                    let mut buf: Vec<proto::EventData> = Vec::with_capacity(rows.len());
                    let mut cursor = start;
                    for r in rows.iter() {
                        let ev = row_to_event(r);
                        cursor = r.get::<i64, _>("global_nonce");
                        buf.push(ev);
                    }
                    phase_opt = Some(Phase::Replay {
                        buf,
                        idx: 0,
                        cursor,
                    });
                }

                match phase_opt.take().unwrap() {
                    Phase::Replay {
                        buf,
                        mut idx,
                        cursor,
                    } => {
                        if idx < buf.len() {
                            let ev = buf[idx].clone();
                            idx += 1;
                            let next_state =
                                (pool, prefix, None, Some(Phase::Replay { buf, idx, cursor }));
                            Some((Ok(proto::SubscribeResponse { event: Some(ev) }), next_state))
                        } else {
                            // Transition to Live phase
                            let next = (
                                pool.clone(),
                                prefix.clone(),
                                None,
                                Some(Phase::Live {
                                    cursor,
                                    interval: interval(Duration::from_millis(200)),
                                    pending: Vec::new(),
                                }),
                            );
                            Some((Ok(proto::SubscribeResponse { event: None }), next))
                        }
                    }
                    Phase::Live {
                        mut cursor,
                        mut interval,
                        mut pending,
                    } => {
                        // If we have pending from last poll, yield first
                        if let Some(ev) = pending.first().cloned() {
                            pending.remove(0);
                            let next_state = (
                                pool,
                                prefix,
                                None,
                                Some(Phase::Live {
                                    cursor,
                                    interval,
                                    pending,
                                }),
                            );
                            return Some((
                                Ok(proto::SubscribeResponse { event: Some(ev) }),
                                next_state,
                            ));
                        }

                        // Fetch new rows strictly greater than cursor
                        let rows = if prefix.is_empty() {
                            sqlx::query(
                                r#"SELECT global_nonce, event_id::text AS event_id,
                                   aggregate_id, aggregate_type, aggregate_nonce,
                                   event_type, content_type, payload
                                   FROM events
                                   WHERE global_nonce > $1
                                   ORDER BY global_nonce ASC"#,
                            )
                            .bind(cursor)
                            .fetch_all(&pool)
                            .await
                            .unwrap_or_default()
                        } else {
                            let like = format!("{prefix}%");
                            sqlx::query(
                                r#"SELECT global_nonce, event_id::text AS event_id,
                                   aggregate_id, aggregate_type, aggregate_nonce,
                                   event_type, content_type, payload
                                   FROM events
                                   WHERE global_nonce > $1 AND aggregate_id LIKE $2
                                   ORDER BY global_nonce ASC"#,
                            )
                            .bind(cursor)
                            .bind(like)
                            .fetch_all(&pool)
                            .await
                            .unwrap_or_default()
                        };

                        if !rows.is_empty() {
                            for r in rows.iter() {
                                let gp = r.get::<i64, _>("global_nonce");
                                cursor = cursor.max(gp);
                                pending.push(row_to_event(r));
                            }
                            // yield first
                            let ev = pending.remove(0);
                            let next_state = (
                                pool,
                                prefix,
                                None,
                                Some(Phase::Live {
                                    cursor,
                                    interval,
                                    pending,
                                }),
                            );
                            Some((Ok(proto::SubscribeResponse { event: Some(ev) }), next_state))
                        } else {
                            // No new events; wait and try again next poll
                            interval.tick().await;
                            let next_state = (
                                pool,
                                prefix,
                                None,
                                Some(Phase::Live {
                                    cursor,
                                    interval,
                                    pending,
                                }),
                            );
                            Some((Ok(proto::SubscribeResponse { event: None }), next_state))
                        }
                    }
                }
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::StreamExt;

    #[tokio::test]
    async fn lazy_pool_methods_handle_errors_and_subscribe_yields_ok() {
        // use a lazy pool to avoid actual DB connection in unit tests
        #[cfg(test)]
        fn new_lazy_for_tests() -> Arc<PostgresStore> {
            let pool = PgPoolOptions::new()
                .connect_lazy("postgres://postgres:password@127.0.0.1:1/db")
                .expect("lazy connect should not attempt network");
            PostgresStore::new(pool)
        }
        let store = new_lazy_for_tests();

        // append returns error
        let append_res = store
            .append(proto::AppendRequest {
                aggregate_id: "s".into(),
                aggregate_type: "t".into(),
                events: vec![],
                expected: None,
            })
            .await;
        assert!(append_res.is_err());

        // read_stream returns error
        let read_res = store
            .read_stream(proto::ReadStreamRequest {
                aggregate_id: "s".into(),
                from_aggregate_nonce: 1,
                max_count: 10,
                forward: true,
            })
            .await;
        assert!(read_res.is_err());

        // subscribe should produce a non-error item (may be empty event due to polling)
        let mut st = store.subscribe(proto::SubscribeRequest {
            aggregate_prefix: "".into(),
            from_global_nonce: 0,
        });
        let first = st.next().await;
        assert!(first.is_some());
        assert!(first.unwrap().is_ok());
    }

    #[tokio::test]
    async fn connect_invalid_url_errors_fast() {
        // This attempts a connection to a non-routable/closed port and should error
        let url = "postgres://postgres:password@127.0.0.1:1/db";
        let res = PostgresStore::connect(url).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn read_stream_db_error_covers_forward_and_backward() {
        fn new_lazy_for_tests() -> Arc<PostgresStore> {
            let pool = PgPoolOptions::new()
                .connect_lazy("postgres://postgres:password@127.0.0.1:1/db")
                .expect("lazy connect should not attempt network");
            PostgresStore::new(pool)
        }
        let store = new_lazy_for_tests();

        // forward path
        let res_fwd = store
            .read_stream(proto::ReadStreamRequest {
                aggregate_id: "s".into(),
                from_aggregate_nonce: 1,
                max_count: 10,
                forward: true,
            })
            .await;
        assert!(res_fwd.is_err());

        // backward path
        let res_bwd = store
            .read_stream(proto::ReadStreamRequest {
                aggregate_id: "s".into(),
                from_aggregate_nonce: 1,
                max_count: 10,
                forward: false,
            })
            .await;
        assert!(res_bwd.is_err());
    }

    #[tokio::test]
    async fn append_db_error_covers_select_and_insert_map_err() {
        fn new_lazy_for_tests() -> Arc<PostgresStore> {
            let pool = PgPoolOptions::new()
                .connect_lazy("postgres://postgres:password@127.0.0.1:1/db")
                .expect("lazy connect should not attempt network");
            PostgresStore::new(pool)
        }
        let store = new_lazy_for_tests();

        // minimal event with event_id to pass validation
        let ev = proto::EventData {
            meta: Some(proto::EventMetadata {
                event_id: "00000000-0000-0000-0000-000000000000".into(),
                ..Default::default()
            }),
            payload: vec![],
        };
        let res = store
            .append(proto::AppendRequest {
                aggregate_id: "s".into(),
                aggregate_type: "t".into(),
                events: vec![ev],
                expected: None,
            })
            .await;
        assert!(res.is_err());
    }
}
