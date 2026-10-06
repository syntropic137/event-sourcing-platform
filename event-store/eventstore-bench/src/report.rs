//! Run results (JSON) and the Markdown summary printed after a run.

use std::fmt::Write as _;

use serde::Serialize;

use crate::e2e::E2eResult;
use crate::env::Environment;
use crate::replay::{CatchupResult, ReadAllResult, ReadStreamResult};
use crate::sampler::Resources;
use crate::workload::{AppendResult, Mode, PreloadResult};

#[derive(Debug, Clone, Serialize)]
pub struct Results {
    pub profile: String,
    pub durability: String,
    pub started_unix_s: u64,
    pub total_secs: f64,
    pub env: Environment,
    pub appends: Vec<AppendResult>,
    pub preloads: Vec<PreloadResult>,
    pub read_all: Vec<ReadAllResult>,
    pub read_stream: Vec<ReadStreamResult>,
    pub catchup: Vec<CatchupResult>,
    pub e2e: Vec<E2eResult>,
}

impl Results {
    /// True when every completeness/ordering check passed and no request
    /// failed. Latency is recorded for successful requests only, so a run
    /// with errors would publish a success-only tail; it fails instead.
    pub fn all_verified(&self) -> bool {
        self.appends.iter().all(|a| a.verify.ok && a.errors == 0)
            && self.preloads.iter().all(|p| p.errors == 0)
            && self.read_all.iter().all(|r| r.verify.ok)
            && self.read_stream.iter().all(|r| r.invalid == 0)
            && self.catchup.iter().all(|c| c.check.exact)
            && self
                .e2e
                .iter()
                .all(|e| e.verify.ok && e.subscribers_exact && e.append_errors == 0)
    }
}

fn f0(v: f64) -> String {
    format!("{v:.0}")
}

fn f1(v: f64) -> String {
    format!("{v:.1}")
}

fn f2(v: f64) -> String {
    format!("{v:.2}")
}

fn opt(v: Option<f64>) -> String {
    v.map(f0).unwrap_or_else(|| "-".into())
}

fn res(r: &Resources) -> String {
    format!(
        "{} / {} / {}",
        opt(r.server_cpu_pct),
        opt(r.pg_cpu_pct),
        opt(r.server_rss_peak_mb)
    )
}

fn ok(b: bool) -> &'static str {
    if b {
        "pass"
    } else {
        "FAIL"
    }
}

pub fn markdown(r: &Results) -> String {
    let mut s = String::new();
    let e = &r.env;
    let _ = writeln!(
        s,
        "## Run: profile `{}`, durability `{}`, {:.0}s, all checks: **{}**\n",
        r.profile,
        r.durability,
        r.total_secs,
        ok(r.all_verified())
    );
    let _ = writeln!(
        s,
        "- Host: {} ({} logical CPUs, {} GB), {}\n- Docker {} VM: {} CPUs, {} GB; Postgres container `{}`: {} CPUs, {} GB\n- Postgres {}: fsync={} synchronous_commit={} full_page_writes={} wal_sync_method={} shared_buffers={} max_connections={}\n- Server pool max {} connections; git {}{}; {}\n",
        e.host_cpu,
        e.host_logical_cpus,
        e.host_mem_gb,
        e.os,
        e.docker_server,
        e.docker_vm_cpus,
        e.docker_vm_mem_gb,
        e.pg_image,
        e.pg_container_cpus,
        e.pg_container_mem_gb,
        e.pg_settings.get("server_version").map(String::as_str).unwrap_or("?"),
        e.pg_settings.get("fsync").map(String::as_str).unwrap_or("?"),
        e.pg_settings.get("synchronous_commit").map(String::as_str).unwrap_or("?"),
        e.pg_settings.get("full_page_writes").map(String::as_str).unwrap_or("?"),
        e.pg_settings.get("wal_sync_method").map(String::as_str).unwrap_or("?"),
        e.pg_settings.get("shared_buffers").map(String::as_str).unwrap_or("?"),
        e.pg_settings.get("max_connections").map(String::as_str).unwrap_or("?"),
        e.server_pool_max,
        &e.git_sha[..e.git_sha.len().min(12)],
        if e.git_dirty { " (dirty)" } else { "" },
        e.rustc,
    );

    let _ = writeln!(s, "### Append\n");
    let _ = writeln!(
        s,
        "Closed loop: latency is service time at saturation. Open loop: latency from intended send (coordinated-omission corrected). CPU% 100 = one core. Pool busy = % of samples with all {} pool connections checked out. Adv wait = backends waiting on the per-tenant ordering lock (mean / max).\n",
        e.server_pool_max
    );
    let _ = writeln!(s, "| scenario | writers | tenants | batch | payload B | mode | events/s | req/s | p50 ms | p95 ms | p99 ms | p99.9 ms | max ms | err | pool busy % | adv wait | WAL wait | srv CPU / pg CPU / srv RSS MB | DB events before | verified |");
    let _ = writeln!(
        s,
        "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"
    );
    for a in &r.appends {
        let sp = &a.spec;
        let mode = match sp.mode {
            Mode::Closed => "closed".to_string(),
            Mode::Open { rps } => format!("open {rps:.0} rps"),
        };
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} / {} | {} | {} | {} | {} |",
            sp.name,
            sp.writers,
            if sp.same_tenant { "1".to_string() } else { sp.writers.to_string() },
            sp.batch,
            sp.payload,
            mode,
            f0(a.events_per_sec),
            f0(a.requests_per_sec),
            f2(a.latency.p50_ms),
            f2(a.latency.p95_ms),
            f2(a.latency.p99_ms),
            f2(a.latency.p999_ms),
            f1(a.latency.max_ms),
            a.errors,
            f0(a.pg.pool_all_busy_pct),
            f2(a.pg.advisory_wait_mean),
            a.pg.advisory_wait_max,
            f2(a.pg.wal_io_wait_mean),
            res(&a.resources),
            a.db_events_before,
            ok(a.verify.ok),
        );
    }

    if !r.preloads.is_empty() {
        let _ = writeln!(s, "\n### History preload\n");
        let _ = writeln!(
            s,
            "| tenant total | added | writers | batch | payload B | secs | events/s | errors |"
        );
        let _ = writeln!(s, "|---|---|---|---|---|---|---|---|");
        for p in &r.preloads {
            let _ = writeln!(
                s,
                "| {} | {} | {} | {} | {} | {} | {} | {} |",
                p.total,
                p.added,
                p.writers,
                p.batch,
                p.payload,
                f1(p.secs),
                f0(p.events_per_sec),
                p.errors
            );
        }
    }

    if !r.read_all.is_empty() {
        let _ = writeln!(
            s,
            "\n### Replay: paged ReadAll (page {})\n",
            crate::replay::READ_ALL_PAGE
        );
        let _ = writeln!(s, "| history | reps | events/s (median scan) | page p50 ms | page p95 ms | page p99 ms | page max ms | srv CPU / pg CPU / srv RSS MB | verified |");
        let _ = writeln!(s, "|---|---|---|---|---|---|---|---|---|");
        for x in &r.read_all {
            let _ = writeln!(
                s,
                "| {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                x.history,
                x.reps,
                f0(x.events_per_sec_median),
                f2(x.page_latency.p50_ms),
                f2(x.page_latency.p95_ms),
                f2(x.page_latency.p99_ms),
                f1(x.page_latency.max_ms),
                res(&x.resources),
                ok(x.verify.ok)
            );
        }
    }

    if !r.read_stream.is_empty() {
        let _ = writeln!(
            s,
            "\n### Replay: aggregate rehydration (ReadStream, 100 events)\n"
        );
        let _ = writeln!(s, "| history | readers | rehydrations/s | events/s | p50 ms | p95 ms | p99 ms | max ms | pool busy % | srv CPU / pg CPU / srv RSS MB | invalid |");
        let _ = writeln!(s, "|---|---|---|---|---|---|---|---|---|---|---|");
        for x in &r.read_stream {
            let _ = writeln!(
                s,
                "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                x.history,
                x.readers,
                f0(x.rehydrations_per_sec),
                f0(x.events_per_sec),
                f2(x.latency.p50_ms),
                f2(x.latency.p95_ms),
                f2(x.latency.p99_ms),
                f1(x.latency.max_ms),
                f0(x.pg.pool_all_busy_pct),
                res(&x.resources),
                x.invalid
            );
        }
    }

    if !r.catchup.is_empty() {
        let _ = writeln!(s, "\n### Subscription catch-up from 0\n");
        let _ = writeln!(s, "| history | first event ms | caught up ms | events/s | srv CPU / pg CPU / srv RSS MB | exact order |");
        let _ = writeln!(s, "|---|---|---|---|---|---|");
        for c in &r.catchup {
            let _ = writeln!(
                s,
                "| {} | {} | {} | {} | {} | {} |",
                c.history,
                f0(c.first_event_ms),
                f0(c.caught_up_ms),
                f0(c.events_per_sec),
                res(&c.resources),
                ok(c.check.exact)
            );
        }
    }

    if !r.e2e.is_empty() {
        let _ = writeln!(s, "\n### End-to-end subscription delivery\n");
        let _ = writeln!(s, "Delivery latency = receive minus intended append send. Slow subscribers sleep per event.\n");
        let _ = writeln!(s, "| scenario | subs (fast+slow) | target ev/s | appended ev/s | append p99 ms | deliver p50 ms | p95 ms | p99 ms | max ms | per-sub ev/s | max lag fast / slow | drain ms fast / slow | slow p99 ms | pool busy % | srv CPU / pg CPU / srv RSS MB | exact order |");
        let _ = writeln!(
            s,
            "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"
        );
        for x in &r.e2e {
            let sp = &x.spec;
            let has_slow = sp.slow_subscribers > 0;
            let slow = |v: String| if has_slow { v } else { "-".into() };
            let _ = writeln!(
                s,
                "| {} | {}+{} | {} | {} | {} | {} | {} | {} | {} | {} | {} / {} | {} / {} | {} | {} | {} | {} |",
                sp.name,
                sp.fast_subscribers,
                sp.slow_subscribers,
                f0(sp.rate_eps),
                f0(x.append_events_per_sec),
                f2(x.append_latency.p99_ms),
                f2(x.delivery.p50_ms),
                f2(x.delivery.p95_ms),
                f2(x.delivery.p99_ms),
                f1(x.delivery.max_ms),
                f0(x.delivered_per_sub_eps),
                x.max_lag_fast,
                slow(x.max_lag_slow.to_string()),
                f0(x.drain_ms_fast),
                slow(f0(x.drain_ms_slow)),
                slow(f1(x.slow_delivery.p99_ms)),
                f0(x.pg.pool_all_busy_pct),
                res(&x.resources),
                ok(x.verify.ok && x.subscribers_exact)
            );
        }
    }
    s
}
