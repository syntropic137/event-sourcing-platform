//! Replay paths over a preloaded history: paged `ReadAll` scans, aggregate
//! rehydration via `ReadStream`, and subscription catch-up from zero.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::sampler::{PgActivity, Resources};
use crate::stats::{self, Latency};
use crate::verify::{self, SeqCheck, Verify};
use crate::workload::{Window, EVENTS_PER_AGGREGATE};
use crate::{pb, Ctx};

pub const READ_ALL_PAGE: u32 = 1000;

#[derive(Debug, Clone, Serialize)]
pub struct ReadAllResult {
    pub history: u64,
    pub page_size: u32,
    pub reps: usize,
    /// Best and median full-scan rate across reps.
    pub events_per_sec_median: f64,
    pub scan_secs: Vec<f64>,
    pub page_latency: Latency,
    pub pg: PgActivity,
    pub resources: Resources,
    /// Committed history vs acknowledged preload appends.
    pub verify: Verify,
}

/// Pages the tenant `reps` times. The first scan also verifies history.
pub async fn read_all_scan(
    ctx: &Ctx,
    tenant: &str,
    acked: &HashSet<String>,
    reps: usize,
) -> anyhow::Result<(ReadAllResult, Vec<u64>)> {
    let mut client = ctx.client().await?;
    let mut hist = stats::new_hist();
    let mut scan_secs = Vec::with_capacity(reps);
    let mut stored = Vec::new();
    let mon = ctx.monitors(Instant::now());
    for rep in 0..reps {
        let t0 = Instant::now();
        let mut from = 0u64;
        loop {
            let p0 = Instant::now();
            let page = client
                .read_all(pb::ReadAllRequest {
                    tenant_id: tenant.to_owned(),
                    from_global_nonce: from,
                    max_count: READ_ALL_PAGE,
                    forward: true,
                })
                .await?
                .into_inner();
            stats::record(&mut hist, p0.elapsed());
            if rep == 0 {
                stored.extend(page.events.iter().filter_map(verify::to_stored));
            }
            if page.is_end || page.events.is_empty() {
                break;
            }
            from = page.next_from_global_nonce;
        }
        scan_secs.push(t0.elapsed().as_secs_f64());
    }
    let (pg, resources) = mon.stop().await;
    let v = verify::check(&stored, acked, &HashSet::new());
    let mut sorted = scan_secs.clone();
    sorted.sort_by(f64::total_cmp);
    let median = sorted[sorted.len() / 2];
    let seq = stored.iter().map(|s| s.global).collect();
    Ok((
        ReadAllResult {
            history: stored.len() as u64,
            page_size: READ_ALL_PAGE,
            reps,
            events_per_sec_median: stored.len() as f64 / median,
            scan_secs,
            page_latency: Latency::from_hist(&hist),
            pg,
            resources,
            verify: v,
        },
        seq,
    ))
}

#[derive(Debug, Clone, Serialize)]
pub struct ReadStreamResult {
    pub history: u64,
    pub readers: usize,
    pub events_per_aggregate: u64,
    pub rehydrations_per_sec: f64,
    pub events_per_sec: f64,
    pub latency: Latency,
    /// Reads that errored or returned a non-contiguous / short stream.
    pub invalid: u64,
    pub pg: PgActivity,
    pub resources: Resources,
}

/// Closed-loop full-aggregate reads (100 events each) by `readers` workers.
pub async fn read_stream_bench(
    ctx: &Ctx,
    tenant: &str,
    history: u64,
    aggregates: Arc<Vec<String>>,
    readers: usize,
    warmup_s: f64,
    duration_s: f64,
) -> anyhow::Result<ReadStreamResult> {
    anyhow::ensure!(!aggregates.is_empty(), "no complete aggregates to read");
    let w = Window::new(warmup_s, duration_s);
    let mon = ctx.monitors(w.warm_end);
    let mut handles = Vec::new();
    for r in 0..readers {
        let mut client = ctx.client().await?;
        let aggs = aggregates.clone();
        let tenant = tenant.to_owned();
        handles.push(tokio::spawn(async move {
            let mut h = stats::new_hist();
            let mut ok = 0u64;
            let mut invalid = 0u64;
            let mut x: u64 = 0x9E37_79B9_7F4A_7C15 ^ (r as u64 + 1);
            loop {
                let t0 = Instant::now();
                if t0 >= w.end {
                    break;
                }
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                let agg = &aggs[(x % aggs.len() as u64) as usize];
                let res = client
                    .read_stream(pb::ReadStreamRequest {
                        tenant_id: tenant.clone(),
                        aggregate_id: agg.clone(),
                        from_aggregate_nonce: 1,
                        max_count: 1000,
                        forward: true,
                    })
                    .await;
                let t1 = Instant::now();
                let valid = match &res {
                    Ok(r) => {
                        let ev = &r.get_ref().events;
                        ev.len() as u64 == EVENTS_PER_AGGREGATE
                            && ev.iter().enumerate().all(|(i, e)| {
                                e.meta.as_ref().map(|m| m.aggregate_nonce) == Some(i as u64 + 1)
                            })
                    }
                    Err(_) => false,
                };
                if t0 >= w.warm_end {
                    if valid {
                        stats::record(&mut h, t1 - t0);
                        ok += 1;
                    } else {
                        invalid += 1;
                    }
                }
            }
            (h, ok, invalid)
        }));
    }
    let mut hist = stats::new_hist();
    let mut ok = 0;
    let mut invalid = 0;
    for h in handles {
        let (hh, o, i) = h.await?;
        stats::merge(&mut hist, &hh);
        ok += o;
        invalid += i;
    }
    let (pg, resources) = mon.stop().await;
    Ok(ReadStreamResult {
        history,
        readers,
        events_per_aggregate: EVENTS_PER_AGGREGATE,
        rehydrations_per_sec: ok as f64 / duration_s,
        events_per_sec: (ok * EVENTS_PER_AGGREGATE) as f64 / duration_s,
        latency: Latency::from_hist(&hist),
        invalid,
        pg,
        resources,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct CatchupResult {
    pub history: u64,
    /// Subscribe call to first event (includes the server's initial query).
    pub first_event_ms: f64,
    /// Subscribe call to the caught-up marker.
    pub caught_up_ms: f64,
    pub events_per_sec: f64,
    pub check: SeqCheck,
    pub resources: Resources,
    pub pg: PgActivity,
}

/// Subscribes from global nonce 0 and drains the replay to the caught-up
/// marker, comparing the delivered order to the store's.
pub async fn catchup(
    ctx: &Ctx,
    tenant: &str,
    store_seq: &[u64],
    timeout: Duration,
) -> anyhow::Result<CatchupResult> {
    let mut client = ctx.client().await?;
    let mon = ctx.monitors(Instant::now());
    let t0 = Instant::now();
    let mut stream = client
        .subscribe(pb::SubscribeRequest {
            tenant_id: tenant.to_owned(),
            aggregate_id_prefix: String::new(),
            from_global_nonce: 0,
        })
        .await?
        .into_inner();
    let mut seen = Vec::with_capacity(store_seq.len());
    let mut first = None;
    let deadline = t0 + timeout;
    loop {
        let msg = tokio::time::timeout_at(deadline.into(), stream.message()).await;
        match msg {
            Ok(Ok(Some(m))) => match m.event {
                Some(ev) => {
                    first.get_or_insert_with(Instant::now);
                    if let Some(meta) = ev.meta {
                        seen.push(meta.global_nonce);
                    }
                }
                None => break,
            },
            Ok(Ok(None)) => anyhow::bail!("subscription closed before catch-up marker"),
            Ok(Err(e)) => anyhow::bail!("subscription error: {e}"),
            Err(_) => anyhow::bail!("catch-up did not finish within {timeout:?}"),
        }
    }
    let done = t0.elapsed();
    drop(stream);
    let (pg, resources) = mon.stop().await;
    Ok(CatchupResult {
        history: store_seq.len() as u64,
        first_event_ms: first
            .map(|f| f.duration_since(t0).as_secs_f64() * 1000.0)
            .unwrap_or(0.0),
        caught_up_ms: done.as_secs_f64() * 1000.0,
        events_per_sec: seen.len() as f64 / done.as_secs_f64(),
        check: verify::compare_sequences(store_seq, &seen),
        resources,
        pg,
    })
}
