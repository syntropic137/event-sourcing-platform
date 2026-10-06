//! End-to-end delivery: open-loop appends to one tenant while live
//! subscribers (some optionally slow) consume it.
//!
//! Delivery latency = subscriber receive time minus the append's *intended*
//! send time (embedded in the payload), so writer-side backlog is included.
//! After the run every subscriber's delivered sequence must equal the
//! store's committed order for the tenant, exactly.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::sampler::{PgActivity, Resources};
use crate::stats::{self, Hist, Latency};
use crate::verify::{self, SeqCheck, Verify};
use crate::workload::{self, Window};
use crate::{payload_stamp, pb, Ctx};

#[derive(Debug, Clone, Serialize)]
pub struct E2eSpec {
    pub name: String,
    pub fast_subscribers: usize,
    pub slow_subscribers: usize,
    /// Processing delay a slow subscriber spends per event.
    pub slow_delay_ms: u64,
    pub writers: usize,
    /// Target append rate in events per second (batch size 1).
    pub rate_eps: f64,
    pub payload: usize,
    pub warmup_s: f64,
    pub duration_s: f64,
    pub drain_timeout_s: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct E2eResult {
    pub spec: E2eSpec,
    pub append_events_per_sec: f64,
    /// Open-loop append latency (CO-corrected).
    pub append_latency: Latency,
    pub append_errors: u64,
    /// Fast subscribers, all deliveries merged.
    pub delivery: Latency,
    pub slow_delivery: Latency,
    /// Mean per fast subscriber over the window.
    pub delivered_per_sub_eps: f64,
    pub max_lag_fast: u64,
    pub max_lag_slow: u64,
    /// Last delivery minus last append completion (max over subscribers).
    pub drain_ms_fast: f64,
    pub drain_ms_slow: f64,
    pub subscriber_errors: Vec<String>,
    pub subscribers_exact: bool,
    pub subscriber_checks: Vec<SeqCheck>,
    pub verify: Verify,
    pub pg: PgActivity,
    pub resources: Resources,
}

struct SubOut {
    slow: bool,
    lat: Hist,
    seen: Vec<u64>,
    in_window: u64,
    last_event: Option<Instant>,
    error: Option<String>,
}

#[allow(clippy::too_many_arguments)]
async fn subscriber(
    ctx: &Ctx,
    tenant: String,
    slow: Option<Duration>,
    ready: tokio::sync::oneshot::Sender<()>,
    received: Arc<AtomicU64>,
    expected: Arc<AtomicU64>,
    give_up: Arc<AtomicBool>,
    window: Arc<std::sync::Mutex<Option<Window>>>,
) -> anyhow::Result<tokio::task::JoinHandle<SubOut>> {
    let mut client = ctx.client().await?;
    let epoch = ctx.epoch;
    Ok(tokio::spawn(async move {
        let mut out = SubOut {
            slow: slow.is_some(),
            lat: stats::new_hist(),
            seen: Vec::new(),
            in_window: 0,
            last_event: None,
            error: None,
        };
        let stream = client
            .subscribe(pb::SubscribeRequest {
                tenant_id: tenant,
                aggregate_id_prefix: String::new(),
                from_global_nonce: 0,
            })
            .await;
        let mut stream = match stream {
            Ok(s) => s.into_inner(),
            Err(e) => {
                out.error = Some(e.to_string());
                return out;
            }
        };
        // Empty tenant: the first message is the caught-up marker.
        match stream.message().await {
            Ok(Some(m)) if m.event.is_none() => {}
            other => {
                out.error = Some(format!("expected caught-up marker, got {other:?}"));
                return out;
            }
        }
        let _ = ready.send(());
        let mut win: Option<Window> = None;
        loop {
            if out.seen.len() as u64 >= expected.load(Ordering::Acquire)
                || give_up.load(Ordering::Relaxed)
            {
                break;
            }
            let msg = tokio::time::timeout(Duration::from_millis(200), stream.message()).await;
            let m = match msg {
                Err(_) => continue,
                Ok(Ok(Some(m))) => m,
                Ok(Ok(None)) => {
                    out.error = Some("stream ended".into());
                    break;
                }
                Ok(Err(e)) => {
                    out.error = Some(e.to_string());
                    break;
                }
            };
            let Some(ev) = m.event else { continue };
            let now = Instant::now();
            if win.is_none() {
                win = *window.lock().expect("window lock");
            }
            if let (Some(meta), Some(stamp), Some(w)) = (&ev.meta, payload_stamp(&ev.payload), win)
            {
                let intended = epoch + Duration::from_nanos(stamp);
                if intended >= w.warm_end && intended < w.end {
                    stats::record(&mut out.lat, now.saturating_duration_since(intended));
                    out.in_window += 1;
                }
                out.seen.push(meta.global_nonce);
            }
            out.last_event = Some(now);
            received.fetch_add(1, Ordering::Relaxed);
            if let Some(d) = slow {
                tokio::time::sleep(d).await;
            }
        }
        out
    }))
}

pub async fn run_e2e(ctx: &Ctx, spec: E2eSpec) -> anyhow::Result<E2eResult> {
    let tenant = ctx.tenant(&spec.name);
    let expected = Arc::new(AtomicU64::new(u64::MAX));
    let give_up = Arc::new(AtomicBool::new(false));
    let window = Arc::new(std::sync::Mutex::new(None));
    let total_subs = spec.fast_subscribers + spec.slow_subscribers;
    let mut subs = Vec::new();
    let mut counters = Vec::new();
    let mut readies = Vec::new();
    for i in 0..total_subs {
        let slow = (i >= spec.fast_subscribers).then(|| Duration::from_millis(spec.slow_delay_ms));
        let (tx, rx) = tokio::sync::oneshot::channel();
        let c = Arc::new(AtomicU64::new(0));
        subs.push(
            subscriber(
                ctx,
                tenant.clone(),
                slow,
                tx,
                c.clone(),
                expected.clone(),
                give_up.clone(),
                window.clone(),
            )
            .await?,
        );
        counters.push((slow.is_some(), c));
        readies.push(rx);
    }
    for r in readies {
        tokio::time::timeout(Duration::from_secs(30), r)
            .await
            .map_err(|_| anyhow::anyhow!("subscriber not ready in 30s"))?
            .map_err(|_| anyhow::anyhow!("subscriber failed before ready"))?;
    }

    let writers = workload::make_writers(ctx, &spec.name, spec.writers, true, spec.payload).await?;
    let w = Window::new(spec.warmup_s, spec.duration_s);
    *window.lock().expect("window lock") = Some(w);
    let mon = ctx.monitors(w.warm_end);
    let acked = Arc::new(AtomicU64::new(0));

    let lag_stop = Arc::new(AtomicBool::new(false));
    let lag_task = {
        let acked = acked.clone();
        let counters = counters.clone();
        let stop = lag_stop.clone();
        tokio::spawn(async move {
            let (mut fast, mut slow) = (0u64, 0u64);
            while !stop.load(Ordering::Relaxed) {
                let a = acked.load(Ordering::Relaxed);
                for (is_slow, c) in &counters {
                    let lag = a.saturating_sub(c.load(Ordering::Relaxed));
                    if *is_slow {
                        slow = slow.max(lag);
                    } else {
                        fast = fast.max(lag);
                    }
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            (fast, slow)
        })
    };

    let handles = workload::run_open(ctx, writers, 1, spec.rate_eps, w, acked.clone());
    let c = workload::collect(handles).await?;
    let writers_done = c.last_done.unwrap_or_else(Instant::now);
    let total_acked: u64 = c.writers.iter().map(|w| w.acked.len() as u64).sum();
    let total_uncertain: u64 = c.writers.iter().map(|w| w.uncertain.len() as u64).sum();
    let (pg, resources) = mon.stop().await;
    // Subscribers stop once they have every committed event. With failed
    // appends the committed count is unknown up front, so they run to the
    // drain timeout and the sequence check decides.
    if total_uncertain == 0 {
        expected.store(total_acked, Ordering::Release);
    }
    {
        let give_up = give_up.clone();
        let t = Duration::from_secs_f64(spec.drain_timeout_s);
        tokio::spawn(async move {
            tokio::time::sleep(t).await;
            give_up.store(true, Ordering::Relaxed);
        });
    }
    let mut outs = Vec::new();
    for h in subs {
        outs.push(h.await?);
    }
    lag_stop.store(true, Ordering::Relaxed);
    let (max_lag_fast, max_lag_slow) = lag_task.await?;

    let (verify, seqs) = workload::verify_writers(ctx, &c.writers).await?;
    let store_seq = seqs.get(&tenant).cloned().unwrap_or_default();

    let mut delivery = stats::new_hist();
    let mut slow_delivery = stats::new_hist();
    let mut fast_in_window = 0u64;
    let (mut drain_fast, mut drain_slow) = (0f64, 0f64);
    let mut checks = Vec::new();
    let mut errors = Vec::new();
    for o in &outs {
        let drain = o
            .last_event
            .map(|t| t.saturating_duration_since(writers_done).as_secs_f64() * 1000.0)
            .unwrap_or(0.0);
        if o.slow {
            stats::merge(&mut slow_delivery, &o.lat);
            drain_slow = drain_slow.max(drain);
        } else {
            stats::merge(&mut delivery, &o.lat);
            fast_in_window += o.in_window;
            drain_fast = drain_fast.max(drain);
        }
        if let Some(e) = &o.error {
            errors.push(e.clone());
        }
        checks.push(verify::compare_sequences(&store_seq, &o.seen));
    }
    let exact = checks.iter().all(|c| c.exact) && errors.is_empty();
    Ok(E2eResult {
        append_events_per_sec: c.events as f64 / spec.duration_s,
        append_latency: Latency::from_hist(&c.lat),
        append_errors: c.errors,
        delivery: Latency::from_hist(&delivery),
        slow_delivery: Latency::from_hist(&slow_delivery),
        delivered_per_sub_eps: fast_in_window as f64
            / spec.fast_subscribers.max(1) as f64
            / spec.duration_s,
        max_lag_fast,
        max_lag_slow,
        drain_ms_fast: drain_fast,
        drain_ms_slow: drain_slow,
        subscriber_errors: errors,
        subscribers_exact: exact,
        subscriber_checks: checks,
        verify,
        pg,
        resources,
        spec,
    })
}
