//! Append load: writers, closed-loop and open-loop drivers, history preload.
//!
//! Closed loop (`Mode::Closed`): each writer issues its next append as soon as
//! the previous one returns. This finds sustainable throughput, but its
//! latency hides queueing (coordinated omission), so it is reported as
//! *service time at saturation*.
//!
//! Open loop (`Mode::Open`): a dispatcher emits requests on a fixed schedule
//! regardless of completions; lanes pick them up, and latency is measured
//! from the *intended* send time. Backlog therefore shows up in the latency,
//! which is the coordinated-omission-corrected tail.

use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::{mpsc, Mutex};

use crate::sampler::{PgActivity, Resources};
use crate::stats::{self, Hist, Latency};
use crate::verify::{self, Verify};
use crate::{achieved_secs, make_event, pb, Client, Ctx};

/// Events per aggregate before a writer moves to a fresh aggregate.
pub const EVENTS_PER_AGGREGATE: u64 = 100;

pub struct Writer {
    client: Client,
    pub tenant: String,
    prefix: String,
    agg_idx: u64,
    nonce: u64,
    payload_len: usize,
    pub acked: Vec<String>,
    pub uncertain: Vec<String>,
    /// Aggregates that reached `EVENTS_PER_AGGREGATE` events.
    pub completed: Vec<String>,
}

impl Writer {
    pub fn new(client: Client, tenant: String, prefix: String, payload_len: usize) -> Self {
        Self {
            client,
            tenant,
            prefix,
            agg_idx: 0,
            nonce: 0,
            payload_len,
            acked: Vec::new(),
            uncertain: Vec::new(),
            completed: Vec::new(),
        }
    }

    /// Appends `batch` events to the writer's current aggregate. Only this
    /// writer touches its aggregates, so OCC conflicts indicate a bug.
    pub async fn append(&mut self, batch: usize, stamp_ns: u64) -> Result<(), tonic::Status> {
        let agg = format!("{}-{}", self.prefix, self.agg_idx);
        let mut events = Vec::with_capacity(batch);
        let mut ids = Vec::with_capacity(batch);
        for i in 0..batch as u64 {
            let (ev, id) = make_event(&agg, self.nonce + 1 + i, self.payload_len, stamp_ns);
            events.push(ev);
            ids.push(id);
        }
        let req = pb::AppendRequest {
            tenant_id: self.tenant.clone(),
            aggregate_id: agg.clone(),
            aggregate_type: "BenchAggregate".into(),
            expected_aggregate_nonce: self.nonce,
            idempotency_key: String::new(),
            events,
        };
        match self.client.append(req).await {
            Ok(_) => {
                self.nonce += batch as u64;
                self.acked.extend(ids);
                if self.nonce >= EVENTS_PER_AGGREGATE {
                    self.completed.push(agg);
                    self.agg_idx += 1;
                    self.nonce = 0;
                }
                Ok(())
            }
            Err(e) => {
                // Outcome unknown: the batch may have committed. Abandon the
                // aggregate so the next append cannot hit a stale nonce.
                self.uncertain.extend(ids);
                self.agg_idx += 1;
                self.nonce = 0;
                Err(e)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind")]
pub enum Mode {
    Closed,
    /// Fixed arrival rate in requests per second.
    Open {
        rps: f64,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct AppendSpec {
    pub name: String,
    pub writers: usize,
    pub batch: usize,
    pub payload: usize,
    pub same_tenant: bool,
    pub mode: Mode,
    pub warmup_s: f64,
    pub duration_s: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppendResult {
    pub spec: AppendSpec,
    pub db_events_before: i64,
    pub events_per_sec: f64,
    pub requests_per_sec: f64,
    /// Closed: service time. Open: intended-send to response (CO-corrected).
    pub latency: Latency,
    /// Send to response, excluding client-side queueing (open loop only).
    pub service: Latency,
    pub errors: u64,
    /// Open loop: how long after the window the backlog took to finish.
    pub drain_ms: f64,
    pub pg: PgActivity,
    pub resources: Resources,
    pub verify: Verify,
}

pub struct LaneOut {
    pub lat: Hist,
    pub svc: Hist,
    pub events: u64,
    pub requests: u64,
    pub errors: u64,
    pub last_done: Option<Instant>,
}

impl LaneOut {
    pub fn new() -> Self {
        Self {
            lat: stats::new_hist(),
            svc: stats::new_hist(),
            events: 0,
            requests: 0,
            errors: 0,
            last_done: None,
        }
    }
}

impl Default for LaneOut {
    fn default() -> Self {
        Self::new()
    }
}

/// Window boundaries for one scenario.
#[derive(Clone, Copy)]
pub struct Window {
    pub start: Instant,
    pub warm_end: Instant,
    pub end: Instant,
}

impl Window {
    pub fn new(warmup_s: f64, duration_s: f64) -> Self {
        let start = Instant::now();
        let warm_end = start + Duration::from_secs_f64(warmup_s);
        Self {
            start,
            warm_end,
            end: warm_end + Duration::from_secs_f64(duration_s),
        }
    }
}

pub fn run_closed(
    ctx: &Ctx,
    writers: Vec<Writer>,
    batch: usize,
    w: Window,
) -> Vec<tokio::task::JoinHandle<(Writer, LaneOut)>> {
    writers
        .into_iter()
        .map(|mut wr| {
            let epoch = ctx.epoch;
            tokio::spawn(async move {
                let mut out = LaneOut::new();
                loop {
                    let t0 = Instant::now();
                    if t0 >= w.end {
                        break;
                    }
                    let stamp = t0.saturating_duration_since(epoch).as_nanos() as u64;
                    let r = wr.append(batch, stamp).await;
                    let t1 = Instant::now();
                    if r.is_err() {
                        out.errors += 1;
                    }
                    if t0 >= w.warm_end && r.is_ok() {
                        stats::record(&mut out.lat, t1 - t0);
                        out.events += batch as u64;
                        out.requests += 1;
                        out.last_done = Some(t1);
                    }
                }
                (wr, out)
            })
        })
        .collect()
}

/// Open-loop driver. Returns lane handles plus a counter of acknowledged
/// events (used by the e2e scenario to compute subscriber lag).
pub fn run_open(
    ctx: &Ctx,
    writers: Vec<Writer>,
    batch: usize,
    rps: f64,
    w: Window,
    acked_events: Arc<AtomicU64>,
) -> Vec<tokio::task::JoinHandle<(Writer, LaneOut)>> {
    let (tx, rx) = mpsc::unbounded_channel::<Instant>();
    let rx = Arc::new(Mutex::new(rx));
    let period = Duration::from_secs_f64(1.0 / rps);
    tokio::spawn(async move {
        let mut i: u32 = 0;
        loop {
            let t = w.start + period * i;
            if t >= w.end {
                break;
            }
            tokio::time::sleep_until(t.into()).await;
            if tx.send(t).is_err() {
                break;
            }
            i += 1;
        }
    });
    writers
        .into_iter()
        .map(|mut wr| {
            let rx = rx.clone();
            let epoch = ctx.epoch;
            let acked = acked_events.clone();
            tokio::spawn(async move {
                let mut out = LaneOut::new();
                loop {
                    let next = rx.lock().await.recv().await;
                    let Some(intended) = next else { break };
                    let t0 = Instant::now();
                    let stamp = intended.saturating_duration_since(epoch).as_nanos() as u64;
                    let r = wr.append(batch, stamp).await;
                    let t1 = Instant::now();
                    match r {
                        Ok(()) => {
                            acked.fetch_add(batch as u64, Ordering::Relaxed);
                            if intended >= w.warm_end {
                                stats::record(&mut out.lat, t1 - intended);
                                stats::record(&mut out.svc, t1 - t0);
                                out.events += batch as u64;
                                out.requests += 1;
                            }
                        }
                        Err(_) => out.errors += 1,
                    }
                    out.last_done = Some(t1);
                }
                (wr, out)
            })
        })
        .collect()
}

pub struct Collected {
    pub writers: Vec<Writer>,
    pub lat: Hist,
    pub svc: Hist,
    pub events: u64,
    pub requests: u64,
    pub errors: u64,
    pub last_done: Option<Instant>,
}

pub async fn collect(
    handles: Vec<tokio::task::JoinHandle<(Writer, LaneOut)>>,
) -> anyhow::Result<Collected> {
    let mut c = Collected {
        writers: Vec::new(),
        lat: stats::new_hist(),
        svc: stats::new_hist(),
        events: 0,
        requests: 0,
        errors: 0,
        last_done: None,
    };
    for h in handles {
        let (wr, out) = h.await?;
        stats::merge(&mut c.lat, &out.lat);
        stats::merge(&mut c.svc, &out.svc);
        c.events += out.events;
        c.requests += out.requests;
        c.errors += out.errors;
        c.last_done = c.last_done.max(out.last_done);
        c.writers.push(wr);
    }
    Ok(c)
}

/// Verifies every tenant the writers touched. Returns the merged result and
/// each tenant's committed global-nonce sequence.
pub async fn verify_writers(
    ctx: &Ctx,
    writers: &[Writer],
) -> anyhow::Result<(Verify, BTreeMap<String, Vec<u64>>)> {
    let mut by_tenant: BTreeMap<String, (HashSet<String>, HashSet<String>)> = BTreeMap::new();
    for w in writers {
        let e = by_tenant.entry(w.tenant.clone()).or_default();
        e.0.extend(w.acked.iter().cloned());
        e.1.extend(w.uncertain.iter().cloned());
    }
    let mut client = ctx.client().await?;
    let mut total = Verify::empty_ok();
    let mut seqs = BTreeMap::new();
    for (tenant, (acked, uncertain)) in by_tenant {
        let (v, seq) = verify::verify_tenant(&mut client, &tenant, &acked, &uncertain).await?;
        total.merge(&v);
        seqs.insert(tenant, seq);
    }
    Ok((total, seqs))
}

pub async fn make_writers(
    ctx: &Ctx,
    name: &str,
    n: usize,
    same_tenant: bool,
    payload: usize,
) -> anyhow::Result<Vec<Writer>> {
    let mut v = Vec::with_capacity(n);
    for i in 0..n {
        let tenant = if same_tenant {
            ctx.tenant(name)
        } else {
            ctx.tenant(&format!("{name}-t{i}"))
        };
        v.push(Writer::new(
            ctx.client().await?,
            tenant,
            format!("w{i}"),
            payload,
        ));
    }
    Ok(v)
}

pub async fn run_append(ctx: &Ctx, spec: AppendSpec) -> anyhow::Result<AppendResult> {
    let writers = make_writers(
        ctx,
        &spec.name,
        spec.writers,
        spec.same_tenant,
        spec.payload,
    )
    .await?;
    let db_events_before = ctx.count_events().await?;
    let w = Window::new(spec.warmup_s, spec.duration_s);
    let mon = ctx.monitors(w.warm_end).stop_at(w.end);
    let handles = match spec.mode {
        Mode::Closed => run_closed(ctx, writers, spec.batch, w),
        Mode::Open { rps } => run_open(
            ctx,
            writers,
            spec.batch,
            rps,
            w,
            Arc::new(AtomicU64::new(0)),
        ),
    };
    let c = collect(handles).await?;
    let (pg, resources) = mon.await?;
    let (verify, _) = verify_writers(ctx, &c.writers).await?;
    let drain_ms = c
        .last_done
        .map(|t| t.saturating_duration_since(w.end).as_secs_f64() * 1000.0)
        .unwrap_or(0.0);
    let secs = achieved_secs(w.warm_end, w.end, c.last_done);
    Ok(AppendResult {
        db_events_before,
        events_per_sec: c.events as f64 / secs,
        requests_per_sec: c.requests as f64 / secs,
        latency: Latency::from_hist(&c.lat),
        service: Latency::from_hist(&c.svc),
        errors: c.errors,
        drain_ms,
        pg,
        resources,
        verify,
        spec,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct PreloadResult {
    pub tenant: String,
    pub added: u64,
    pub total: u64,
    pub writers: usize,
    pub batch: usize,
    pub payload: usize,
    pub secs: f64,
    pub events_per_sec: f64,
    pub errors: u64,
}

pub struct PreloadSpec<'a> {
    pub tenant: &'a str,
    /// Unique per call so aggregate ids never collide across preloads.
    pub tag: &'a str,
    pub add: u64,
    pub total_after: u64,
    pub writers: usize,
    pub batch: usize,
    pub payload: usize,
}

/// Appends `add` events to `tenant` as fast as possible (closed loop).
pub async fn preload(
    ctx: &Ctx,
    s: PreloadSpec<'_>,
) -> anyhow::Result<(PreloadResult, Vec<Writer>)> {
    let PreloadSpec {
        tenant,
        tag,
        add,
        total_after,
        writers,
        batch,
        payload,
    } = s;
    let remaining = Arc::new(AtomicU64::new(add));
    let t0 = Instant::now();
    let mut handles = Vec::new();
    for i in 0..writers {
        let mut wr = Writer::new(
            ctx.client().await?,
            tenant.to_owned(),
            format!("{tag}-w{i}"),
            payload,
        );
        let remaining = remaining.clone();
        handles.push(tokio::spawn(async move {
            let mut errors = 0u64;
            while errors < 100 {
                let cur = remaining.load(Ordering::Relaxed);
                if cur == 0 {
                    break;
                }
                let take = cur.min(batch as u64);
                if remaining
                    .compare_exchange(cur, cur - take, Ordering::Relaxed, Ordering::Relaxed)
                    .is_err()
                {
                    continue;
                }
                if wr.append(take as usize, 0).await.is_err() {
                    errors += 1;
                    remaining.fetch_add(take, Ordering::Relaxed);
                }
            }
            (wr, errors)
        }));
    }
    let mut out = Vec::new();
    let mut errors = 0;
    for h in handles {
        let (wr, e) = h.await?;
        errors += e;
        out.push(wr);
    }
    let secs = t0.elapsed().as_secs_f64();
    Ok((
        PreloadResult {
            tenant: tenant.to_owned(),
            added: add,
            total: total_after,
            writers,
            batch,
            payload,
            secs,
            events_per_sec: add as f64 / secs,
            errors,
        },
        out,
    ))
}
