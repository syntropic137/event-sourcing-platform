//! Live-Postgres benchmark harness for the event store (issue #354).
//!
//! Drives the real `eventstore-bin` gRPC server (spawned as a child process,
//! production pool settings) backed by a real Postgres, and measures append,
//! replay and end-to-end subscription delivery while sampling pool, lock,
//! CPU and memory state. Every loaded scenario is followed by a completeness
//! and ordering check of what was committed. See
//! `docs/performance/POSTGRES-BASELINE.md`.

pub mod e2e;
pub mod env;
pub mod replay;
pub mod report;
pub mod sampler;
pub mod stats;
pub mod verify;
pub mod workload;

use std::time::{Duration, Instant};

pub use eventstore_proto::gen as pb;
use sampler::{PgActivity, PgSampler, ResourceSampler, Resources};

pub type Client = pb::event_store_client::EventStoreClient<tonic::transport::Channel>;

/// Shared state for one benchmark run.
pub struct Ctx {
    pub addr: String,
    /// Origin for timestamps embedded in payloads (same process, monotonic).
    pub epoch: Instant,
    pub run_id: String,
    /// `application_name` the server's connections carry.
    pub server_app_name: String,
    pub sampler_pool: sqlx::PgPool,
    pub server_pid: Option<u32>,
    pub pg_container: Option<String>,
}

impl Ctx {
    /// One HTTP/2 connection per call, so workers do not share a connection.
    pub async fn client(&self) -> anyhow::Result<Client> {
        let ch = tonic::transport::Endpoint::from_shared(format!("http://{}", self.addr))?
            .tcp_nodelay(true)
            .connect()
            .await?;
        Ok(Client::new(ch)
            .max_decoding_message_size(512 << 20)
            .max_encoding_message_size(512 << 20))
    }

    pub fn stamp(&self, t: Instant) -> u64 {
        t.saturating_duration_since(self.epoch).as_nanos() as u64
    }

    pub fn unstamp(&self, ns: u64) -> Instant {
        self.epoch + Duration::from_nanos(ns)
    }

    pub fn tenant(&self, name: &str) -> String {
        format!("{}-{}", self.run_id, name)
    }

    pub fn monitors(&self, at: Instant) -> Monitors {
        Monitors {
            pg: PgSampler::start(self.sampler_pool.clone(), self.server_app_name.clone(), at),
            res: ResourceSampler::start(self.server_pid, self.pg_container.clone(), at),
        }
    }

    pub async fn count_events(&self) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar("SELECT count(*) FROM events")
            .fetch_one(&self.sampler_pool)
            .await?)
    }
}

pub struct Monitors {
    pg: PgSampler,
    res: ResourceSampler,
}

impl Monitors {
    pub async fn stop(self) -> (PgActivity, Resources) {
        let (pg, res) = tokio::join!(self.pg.stop(), self.res.stop());
        (pg, res)
    }
}

/// Builds a bench event. The first 8 payload bytes carry the intended send
/// time (ns since `Ctx::epoch`) so subscribers can measure delivery latency.
pub fn make_event(
    aggregate_id: &str,
    nonce: u64,
    payload_len: usize,
    stamp_ns: u64,
) -> (pb::EventData, String) {
    let event_id = format!("{aggregate_id}#{nonce}");
    let mut payload = vec![0x5A; payload_len.max(8)];
    payload[..8].copy_from_slice(&stamp_ns.to_le_bytes());
    let ev = pb::EventData {
        meta: Some(pb::EventMetadata {
            event_id: event_id.clone(),
            aggregate_id: aggregate_id.to_owned(),
            aggregate_type: "BenchAggregate".into(),
            aggregate_nonce: nonce,
            event_type: "BenchEvent".into(),
            event_version: 1,
            content_type: "application/octet-stream".into(),
            ..Default::default()
        }),
        payload,
    };
    (ev, event_id)
}

pub fn payload_stamp(payload: &[u8]) -> Option<u64> {
    Some(u64::from_le_bytes(payload.get(..8)?.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamp_round_trips_through_payload() {
        let (ev, id) = make_event("agg-1", 3, 256, 123_456_789);
        assert_eq!(id, "agg-1#3");
        assert_eq!(ev.payload.len(), 256);
        assert_eq!(payload_stamp(&ev.payload), Some(123_456_789));
        let (ev, _) = make_event("agg-1", 1, 1, 7);
        assert_eq!(ev.payload.len(), 8);
        assert_eq!(payload_stamp(&ev.payload), Some(7));
        assert_eq!(payload_stamp(&[1, 2]), None);
    }
}
