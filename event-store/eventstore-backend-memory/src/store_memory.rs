use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use parking_lot::RwLock;
use prost::Message;
use sha2::{Digest, Sha256};
use tokio::sync::broadcast;
use tokio_stream::{self as ts, StreamExt};

use eventstore_core::{proto, EventStore, StoreError, StoreStream};
use proto::{
    AppendRequest, AppendResponse, ConcurrencyErrorDetail, EventData, ReadStreamRequest,
    ReadStreamResponse, SubscribeRequest, SubscribeResponse,
};

const DEFAULT_CONTENT_TYPE: &str = "application/octet-stream";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct StreamKey {
    tenant_id: String,
    aggregate_id: String,
}

impl StreamKey {
    fn new(tenant_id: &str, aggregate_id: &str) -> Self {
        Self {
            tenant_id: tenant_id.to_owned(),
            aggregate_id: aggregate_id.to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct IdempotencyKey {
    tenant_id: String,
    aggregate_id: String,
    key: String,
}

impl IdempotencyKey {
    fn new(tenant_id: &str, aggregate_id: &str, key: &str) -> Self {
        Self {
            tenant_id: tenant_id.to_owned(),
            aggregate_id: aggregate_id.to_owned(),
            key: key.to_owned(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct StoredBatch {
    fingerprint: Vec<u8>,
    response: AppendResponse,
}

pub struct InMemoryStore {
    pub(crate) streams: RwLock<HashMap<StreamKey, Vec<EventData>>>,
    pub(crate) all: RwLock<Vec<EventData>>,
    pub(crate) next_global: RwLock<u64>,
    pub(crate) idempotency: RwLock<HashMap<IdempotencyKey, StoredBatch>>,
    pub(crate) tx: broadcast::Sender<EventData>,
}

impl InMemoryStore {
    pub fn new() -> Arc<Self> {
        let (tx, _rx) = broadcast::channel(1024);
        Arc::new(Self {
            streams: RwLock::new(HashMap::new()),
            all: RwLock::new(Vec::new()),
            next_global: RwLock::new(1),
            idempotency: RwLock::new(HashMap::new()),
            tx,
        })
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn batch_fingerprint(events: &[EventData]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    for ev in events {
        if let Some(meta) = &ev.meta {
            let mut clone = EventData {
                meta: Some(meta.clone()),
                payload: ev.payload.clone(),
            };
            // recorded_time/global_nonce are server-assigned; ignore to keep client fingerprint stable
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
    mut event: EventData,
    tenant_id: &str,
    aggregate_id: &str,
    aggregate_type: &str,
) -> Result<EventData, StoreError> {
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

#[async_trait]
impl EventStore for InMemoryStore {
    async fn append(&self, req: AppendRequest) -> Result<AppendResponse, StoreError> {
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

        let mut events: Vec<EventData> = Vec::with_capacity(req.events.len());
        for ev in req.events.into_iter() {
            events.push(normalize_event(
                ev,
                &tenant_id,
                &aggregate_id,
                &aggregate_type,
            )?);
        }

        let fingerprint = batch_fingerprint(&events);
        let stream_key = StreamKey::new(&tenant_id, &aggregate_id);
        let idempotency_key = (!req.idempotency_key.is_empty())
            .then(|| IdempotencyKey::new(&tenant_id, &aggregate_id, &req.idempotency_key));

        let mut streams = self.streams.write();
        let stream = streams.entry(stream_key).or_default();
        let current_last_nonce = stream
            .last()
            .and_then(|ev| ev.meta.as_ref().map(|m| m.aggregate_nonce))
            .unwrap_or(0);
        let current_last_global = stream
            .last()
            .and_then(|ev| ev.meta.as_ref().map(|m| m.global_nonce))
            .unwrap_or(0);

        let expected_head = req.expected_aggregate_nonce;
        let expected_ok = if expected_head == 0 {
            current_last_nonce == 0
        } else {
            current_last_nonce == expected_head
        };
        if !expected_ok {
            let detail = ConcurrencyErrorDetail {
                tenant_id,
                aggregate_id,
                actual_last_aggregate_nonce: current_last_nonce,
                actual_last_global_nonce: current_last_global,
            };
            return Err(StoreError::Concurrency {
                message: "append precondition failed".into(),
                detail: Some(detail),
            });
        }

        for (idx, ev) in events.iter().enumerate() {
            let meta = ev
                .meta
                .as_ref()
                .expect("normalized event must have metadata");
            let expected_nonce = current_last_nonce + idx as u64 + 1;
            if meta.aggregate_nonce != expected_nonce {
                return Err(StoreError::Invalid(format!(
                    "event {} aggregate_nonce {} must equal expected {}",
                    idx, meta.aggregate_nonce, expected_nonce
                )));
            }
        }

        let mut idempotency_guard = idempotency_key.as_ref().map(|_| self.idempotency.write());
        if let (Some(key), Some(guard)) = (&idempotency_key, idempotency_guard.as_mut()) {
            if let Some(existing) = guard.get(key) {
                if existing.fingerprint == fingerprint {
                    return Ok(existing.response);
                }
                return Err(StoreError::AlreadyExists(format!(
                    "idempotency key '{}' already used with different payload",
                    key.key
                )));
            }
        }

        let mut assigned_events: Vec<EventData> = Vec::with_capacity(events.len());
        let mut all = self.all.write();
        let mut next_global = self.next_global.write();
        let mut last_global_nonce = current_last_global;
        for mut ev in events.into_iter() {
            let mut meta = ev.meta.take().expect("normalized event must have metadata");
            let now_ms = now_unix_ms();
            meta.recorded_time_unix_ms = now_ms;
            let global_nonce = *next_global;
            *next_global += 1;
            meta.global_nonce = global_nonce;
            ev.meta = Some(meta.clone());
            stream.push(EventData {
                meta: Some(meta.clone()),
                payload: ev.payload.clone(),
            });
            all.push(EventData {
                meta: Some(meta.clone()),
                payload: ev.payload.clone(),
            });
            last_global_nonce = global_nonce;
            assigned_events.push(EventData {
                meta: Some(meta),
                payload: ev.payload,
            });
        }
        drop(next_global);
        drop(all);
        drop(streams);

        let last_committed = assigned_events
            .last()
            .and_then(|ev| ev.meta.as_ref().map(|m| m.aggregate_nonce))
            .unwrap_or(0);

        let response = AppendResponse {
            last_global_nonce,
            last_aggregate_nonce: last_committed,
        };

        if let (Some(key), Some(guard)) = (&idempotency_key, idempotency_guard.as_mut()) {
            guard.insert(
                key.clone(),
                StoredBatch {
                    fingerprint,
                    response,
                },
            );
        }

        drop(idempotency_guard);

        for ev in assigned_events {
            let _ = self.tx.send(ev);
        }

        Ok(response)
    }

    async fn read_stream(&self, req: ReadStreamRequest) -> Result<ReadStreamResponse, StoreError> {
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

        let stream_key = StreamKey::new(&req.tenant_id, &req.aggregate_id);
        let streams = self.streams.read();
        let events = streams.get(&stream_key).cloned().unwrap_or_default();
        if events.is_empty() {
            return Ok(ReadStreamResponse {
                events: vec![],
                is_end: true,
                next_from_aggregate_nonce: if req.forward {
                    req.from_aggregate_nonce.max(1)
                } else {
                    req.from_aggregate_nonce
                },
            });
        }

        let start_nonce = if req.from_aggregate_nonce <= 1 {
            1
        } else {
            req.from_aggregate_nonce
        };

        let mut page: Vec<EventData> = Vec::new();
        if req.forward {
            for ev in events.iter() {
                let nonce = ev
                    .meta
                    .as_ref()
                    .map(|m| m.aggregate_nonce)
                    .unwrap_or_default();
                if nonce >= start_nonce {
                    page.push(ev.clone());
                }
                if page.len() as u32 >= req.max_count && req.max_count > 0 {
                    break;
                }
            }
        } else {
            // For backward reads, iterate in reverse to get most recent first
            for ev in events.iter().rev() {
                let nonce = ev
                    .meta
                    .as_ref()
                    .map(|m| m.aggregate_nonce)
                    .unwrap_or_default();
                if nonce <= start_nonce {
                    page.push(ev.clone());
                }
                if page.len() as u32 >= req.max_count && req.max_count > 0 {
                    break;
                }
            }
        }

        // Note: No need to reverse for backward reads - iter().rev() already
        // returns events in the correct order (most recent first)

        let next_from = if req.forward {
            page.last()
                .and_then(|ev| ev.meta.as_ref().map(|m| m.aggregate_nonce + 1))
                .unwrap_or(start_nonce)
        } else {
            page.first()
                .and_then(|ev| {
                    ev.meta
                        .as_ref()
                        .map(|m| m.aggregate_nonce.saturating_sub(1))
                })
                .unwrap_or(0)
        };
        let stream_end_nonce = events
            .last()
            .and_then(|ev| ev.meta.as_ref().map(|m| m.aggregate_nonce))
            .unwrap_or(0);
        let is_end = if req.forward {
            page.is_empty() || next_from > stream_end_nonce
        } else {
            page.is_empty() || next_from == 0
        };

        Ok(ReadStreamResponse {
            events: page,
            is_end,
            next_from_aggregate_nonce: next_from,
        })
    }

    fn subscribe(&self, req: SubscribeRequest) -> StoreStream<SubscribeResponse> {
        let tenant_id = req.tenant_id.clone();
        let prefix = req.aggregate_id_prefix.clone();
        let from_global = req.from_global_nonce;

        let replay_items: Vec<Result<SubscribeResponse, StoreError>> = self
            .all
            .read()
            .iter()
            .filter(|ev| {
                ev.meta.as_ref().is_some_and(|m| {
                    m.tenant_id == tenant_id
                        && m.global_nonce >= from_global
                        && (prefix.is_empty() || m.aggregate_id.starts_with(&prefix))
                })
            })
            .cloned()
            .map(|event| Ok(SubscribeResponse { event: Some(event) }))
            .collect();

        let replay = ts::iter(replay_items);

        let rx = self.tx.subscribe();
        let live_tenant = tenant_id.clone();
        let live_prefix = prefix.clone();
        let live = ts::wrappers::BroadcastStream::new(rx).filter_map(move |res| {
            let tenant = live_tenant.clone();
            let prefix = live_prefix.clone();
            match res {
                Ok(event) => {
                    let keep = event.meta.as_ref().is_some_and(|m| {
                        m.tenant_id == tenant
                            && m.global_nonce >= from_global
                            && (prefix.is_empty() || m.aggregate_id.starts_with(&prefix))
                    });

                    if keep {
                        Some(Ok(SubscribeResponse { event: Some(event) }))
                    } else {
                        None
                    }
                }
                Err(_) => None,
            }
        });

        Box::pin(replay.chain(live))
    }
}
