use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;
use tokio::sync::broadcast;
use tokio_stream::{self as ts, StreamExt};

use eventstore_core::{proto, EventStore, StoreError, StoreStream};
use proto::{
    append_request, AppendRequest, AppendResponse, EventData, ReadStreamRequest,
    ReadStreamResponse, SubscribeRequest, SubscribeResponse,
};

pub struct InMemoryStore {
    // per-stream ordered events
    pub(crate) streams: RwLock<HashMap<String, Vec<EventData>>>,
    // all events ordered by global position
    pub(crate) all: RwLock<Vec<EventData>>,
    // monotonically increasing global position
    pub(crate) next_global: RwLock<u64>,
    // broadcaster for new events
    pub(crate) tx: broadcast::Sender<EventData>,
}

impl InMemoryStore {
    pub fn new() -> Arc<Self> {
        let (tx, _rx) = broadcast::channel(1024);
        Arc::new(Self {
            streams: RwLock::new(HashMap::new()),
            all: RwLock::new(Vec::new()),
            next_global: RwLock::new(1),
            tx,
        })
    }
}

fn set_global_nonce(mut ev: EventData, global_nonce: u64) -> EventData {
    if let Some(mut meta) = ev.meta.take() {
        meta.global_nonce = global_nonce;
        ev.meta = Some(meta);
    }
    ev
}

#[async_trait]
impl EventStore for InMemoryStore {
    async fn append(&self, req: AppendRequest) -> Result<AppendResponse, StoreError> {
        let aggregate_id = req.aggregate_id.clone();
        let aggregate_type = req.aggregate_type.clone();
        let mut streams = self.streams.write();
        let v = streams.entry(aggregate_id.clone()).or_default();

        // concurrency check
        let expected_ok = match req.expected {
            Some(append_request::Expected::Exact(ex)) => v.len() as u64 == ex,
            Some(append_request::Expected::ExpectedAny(e)) => {
                use proto::Expected;
                match proto::Expected::try_from(e).unwrap_or(Expected::Any) {
                    Expected::Any => true,
                    Expected::NoAggregate => v.is_empty(),
                    Expected::AggregateExists => !v.is_empty(),
                }
            }
            None => true,
        };
        if !expected_ok {
            return Err(StoreError::Concurrency(format!(
                "expected mismatch for aggregate {aggregate_id}"
            )));
        }

        // Validate client-provided aggregate_nonces
        for (i, ev) in req.events.iter().enumerate() {
            let expected_nonce = v.len() as u64 + 1 + i as u64;
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

        let mut all = self.all.write();
        let mut next_global = self.next_global.write();
        let mut last_global = if *next_global > 0 {
            *next_global - 1
        } else {
            0
        };

        for mut ev in req.events.into_iter() {
            let global_nonce = *next_global;
            *next_global += 1;

            // Set aggregate fields and assign global nonce
            let mut meta = ev.meta.take().unwrap_or_else(|| proto::EventMetadata {
                ..Default::default()
            });
            meta.aggregate_id = aggregate_id.clone();
            meta.aggregate_type = aggregate_type.clone();
            ev.meta = Some(meta);

            let ev2 = set_global_nonce(ev, global_nonce);
            v.push(ev2.clone());
            all.push(ev2.clone());
            last_global = global_nonce;
            // ignore send error when no listeners
            let _ = self.tx.send(ev2);
        }

        Ok(AppendResponse {
            next_aggregate_nonce: v.len() as u64,
            last_global_nonce: last_global,
        })
    }

    async fn read_stream(&self, req: ReadStreamRequest) -> Result<ReadStreamResponse, StoreError> {
        let streams = self.streams.read();
        let v = streams.get(&req.aggregate_id).cloned().unwrap_or_default();
        if v.is_empty() {
            return Ok(ReadStreamResponse {
                events: vec![],
                is_end: true,
                next_from_aggregate_nonce: req.from_aggregate_nonce,
            });
        }
        let start = (req.from_aggregate_nonce.saturating_sub(1)) as usize;
        let max = req.max_count as usize;
        let slice: Vec<EventData> = if req.forward {
            v.iter().skip(start).take(max).cloned().collect()
        } else {
            // backward: take from start backwards
            let end = start + 1;
            let begin = end.saturating_sub(max);
            v[begin..end].iter().rev().cloned().collect()
        };
        let next_from = if req.forward {
            req.from_aggregate_nonce.saturating_add(slice.len() as u64)
        } else {
            req.from_aggregate_nonce.saturating_sub(slice.len() as u64)
        };
        let is_end = slice.is_empty() || (req.forward && next_from as usize > v.len());
        Ok(ReadStreamResponse {
            events: slice,
            is_end,
            next_from_aggregate_nonce: next_from,
        })
    }

    fn subscribe(&self, req: SubscribeRequest) -> StoreStream<SubscribeResponse> {
        let from_pos = req.from_global_nonce;
        let prefix_arc = Arc::new(req.aggregate_prefix.clone());

        // Snapshot and build owned replay items
        let replay_items: Vec<Result<SubscribeResponse, StoreError>> = {
            let all = self.all.read();
            let p = prefix_arc.clone();
            all.iter()
                .filter(|e| {
                    let ok_pos = e
                        .meta
                        .as_ref()
                        .map(|m| m.global_nonce >= from_pos)
                        .unwrap_or(false);
                    let ok_prefix = p.is_empty()
                        || e.meta
                            .as_ref()
                            .map(|m| m.aggregate_id.starts_with(&*p))
                            .unwrap_or(false);
                    ok_pos && ok_prefix
                })
                .cloned()
                .map(|e| Ok(SubscribeResponse { event: Some(e) }))
                .collect()
        };
        let replay = ts::iter(replay_items);

        // Live stream from broadcast, using Arc<String> to satisfy 'static
        let rx = self.tx.subscribe();
        let p_live = prefix_arc.clone();
        let live = ts::wrappers::BroadcastStream::new(rx).filter_map(move |res| {
            let p = p_live.clone();
            match res {
                Ok(e) => {
                    let ok_prefix = p.is_empty()
                        || e.meta
                            .as_ref()
                            .map(|m| m.aggregate_id.starts_with(&*p))
                            .unwrap_or(false);
                    if ok_prefix {
                        Some(Ok(SubscribeResponse { event: Some(e) }))
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
