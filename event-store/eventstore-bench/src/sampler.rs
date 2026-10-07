//! Background samplers that run during a scenario's measurement window.
//!
//! * [`PgSampler`] polls `pg_stat_activity` / `pg_locks` for the server's
//!   connections (tagged with `application_name`), giving pool occupancy and
//!   advisory-lock / WAL waits. sqlx does not expose pool acquire waits, so a
//!   sample where every pool connection is checked out is the proxy: any
//!   request arriving then waits for a connection.
//! * [`ResourceSampler`] samples CPU and memory of the server process, the
//!   bench process (`ps`) and the Postgres container (cgroup v2 files via
//!   `docker exec`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use sqlx::PgPool;
use tokio::process::Command;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

/// Mirrors `PgPoolOptions::max_connections(5)` in
/// `PostgresStore::connect` (eventstore-backend-postgres). Keep in sync.
pub const SERVER_POOL_MAX: i64 = 5;

pub const PG_SAMPLE_EVERY: Duration = Duration::from_millis(20);
const RESOURCE_SAMPLE_EVERY: Duration = Duration::from_millis(500);

const ACTIVITY_SQL: &str = r#"
SELECT
  count(*)::bigint,
  count(*) FILTER (WHERE state IS NOT NULL AND state <> 'idle')::bigint,
  count(*) FILTER (WHERE state LIKE 'idle in transaction%')::bigint,
  count(*) FILTER (WHERE wait_event_type = 'Lock' AND wait_event = 'advisory')::bigint,
  count(*) FILTER (WHERE wait_event_type = 'Lock' AND wait_event <> 'advisory')::bigint,
  count(*) FILTER (WHERE wait_event_type = 'IO' AND wait_event ILIKE 'wal%')::bigint,
  (SELECT count(*) FROM pg_locks WHERE locktype = 'advisory' AND NOT granted)::bigint
FROM pg_stat_activity
WHERE application_name = $1 AND backend_type = 'client backend'
"#;

#[derive(Debug, Clone, Default, Serialize)]
pub struct PgActivity {
    pub samples: u64,
    pub sample_errors: u64,
    /// Connections the server held (pool + LISTEN connection).
    pub conns_max: i64,
    /// Connections executing or inside a transaction (checked out of the pool).
    pub busy_mean: f64,
    pub busy_max: i64,
    /// % of samples with all `SERVER_POOL_MAX` pool connections busy.
    pub pool_all_busy_pct: f64,
    /// Connections idle inside an open transaction (server round trips).
    pub idle_in_tx_mean: f64,
    pub advisory_wait_mean: f64,
    pub advisory_wait_max: i64,
    /// % of samples with at least one backend waiting on the ordering lock.
    pub advisory_wait_any_pct: f64,
    pub advisory_ungranted_max: i64,
    pub other_lock_wait_mean: f64,
    /// Backends waiting on WAL write/flush (commit durability cost).
    pub wal_io_wait_mean: f64,
}

#[derive(Default)]
struct ActivityAcc {
    n: u64,
    errors: u64,
    conns_max: i64,
    busy_sum: i64,
    busy_max: i64,
    all_busy: u64,
    idle_tx_sum: i64,
    adv_sum: i64,
    adv_max: i64,
    adv_any: u64,
    ungranted_max: i64,
    other_lock_sum: i64,
    wal_sum: i64,
}

type ActivityRow = (i64, i64, i64, i64, i64, i64, i64);

impl ActivityAcc {
    fn add(&mut self, r: ActivityRow) {
        let (conns, busy, idle_tx, adv, other, wal, ungranted) = r;
        self.n += 1;
        self.conns_max = self.conns_max.max(conns);
        self.busy_sum += busy;
        self.busy_max = self.busy_max.max(busy);
        if busy >= SERVER_POOL_MAX {
            self.all_busy += 1;
        }
        self.idle_tx_sum += idle_tx;
        self.adv_sum += adv;
        self.adv_max = self.adv_max.max(adv);
        if adv > 0 {
            self.adv_any += 1;
        }
        self.ungranted_max = self.ungranted_max.max(ungranted);
        self.other_lock_sum += other;
        self.wal_sum += wal;
    }

    fn finish(self) -> PgActivity {
        let n = self.n.max(1) as f64;
        PgActivity {
            samples: self.n,
            sample_errors: self.errors,
            conns_max: self.conns_max,
            busy_mean: self.busy_sum as f64 / n,
            busy_max: self.busy_max,
            pool_all_busy_pct: 100.0 * self.all_busy as f64 / n,
            idle_in_tx_mean: self.idle_tx_sum as f64 / n,
            advisory_wait_mean: self.adv_sum as f64 / n,
            advisory_wait_max: self.adv_max,
            advisory_wait_any_pct: 100.0 * self.adv_any as f64 / n,
            advisory_ungranted_max: self.ungranted_max,
            other_lock_wait_mean: self.other_lock_sum as f64 / n,
            wal_io_wait_mean: self.wal_sum as f64 / n,
        }
    }
}

pub struct PgSampler {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<PgActivity>,
}

impl PgSampler {
    pub fn start(pool: PgPool, app_name: String, at: Instant) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep_until(at.into()).await;
            let mut acc = ActivityAcc::default();
            let mut tick = tokio::time::interval(PG_SAMPLE_EVERY);
            tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
            while !stop2.load(Ordering::Relaxed) {
                tick.tick().await;
                match sqlx::query_as::<_, ActivityRow>(ACTIVITY_SQL)
                    .bind(&app_name)
                    .fetch_one(&pool)
                    .await
                {
                    Ok(r) => acc.add(r),
                    Err(_) => acc.errors += 1,
                }
            }
            acc.finish()
        });
        Self { stop, handle }
    }

    pub async fn stop(self) -> PgActivity {
        self.stop.store(true, Ordering::Relaxed);
        self.handle.await.unwrap_or_default()
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Resources {
    /// 100 = one full core, averaged over the window.
    pub server_cpu_pct: Option<f64>,
    pub server_rss_peak_mb: Option<f64>,
    pub bench_cpu_pct: Option<f64>,
    pub bench_rss_peak_mb: Option<f64>,
    pub pg_cpu_pct: Option<f64>,
    /// cgroup `memory.current` peak; includes the container's page cache.
    pub pg_mem_peak_mb: Option<f64>,
}

#[derive(Clone, Copy, Default)]
struct ProcSample {
    cpu_s: f64,
    rss_kb: f64,
}

/// (taken at, server, bench, container cpu seconds)
type Point = (Instant, Option<ProcSample>, Option<ProcSample>, Option<f64>);

#[derive(Default)]
struct ResAcc {
    first: Option<Point>,
    last: Option<Point>,
    server_rss_peak: Option<f64>,
    bench_rss_peak: Option<f64>,
    pg_mem_peak: Option<f64>,
}

fn pct(a: Option<ProcSample>, b: Option<ProcSample>, secs: f64) -> Option<f64> {
    match (a, b) {
        (Some(a), Some(b)) if secs > 0.0 => Some(100.0 * (b.cpu_s - a.cpu_s) / secs),
        _ => None,
    }
}

fn max_opt(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (x, None) => x,
        (None, y) => y,
    }
}

impl ResAcc {
    fn add(
        &mut self,
        t: Instant,
        server: Option<ProcSample>,
        bench: Option<ProcSample>,
        pg: Option<(f64, f64)>,
    ) {
        let pg_cpu = pg.map(|p| p.0);
        if self.first.is_none() {
            self.first = Some((t, server, bench, pg_cpu));
        }
        self.last = Some((t, server, bench, pg_cpu));
        self.server_rss_peak = max_opt(self.server_rss_peak, server.map(|s| s.rss_kb / 1024.0));
        self.bench_rss_peak = max_opt(self.bench_rss_peak, bench.map(|s| s.rss_kb / 1024.0));
        self.pg_mem_peak = max_opt(self.pg_mem_peak, pg.map(|p| p.1 / (1024.0 * 1024.0)));
    }

    fn finish(self) -> Resources {
        let (Some(f), Some(l)) = (self.first, self.last) else {
            return Resources::default();
        };
        let secs = l.0.duration_since(f.0).as_secs_f64();
        let pg_cpu_pct = match (f.3, l.3) {
            (Some(a), Some(b)) if secs > 0.0 => Some(100.0 * (b - a) / secs),
            _ => None,
        };
        Resources {
            server_cpu_pct: pct(f.1, l.1, secs),
            server_rss_peak_mb: self.server_rss_peak,
            bench_cpu_pct: pct(f.2, l.2, secs),
            bench_rss_peak_mb: self.bench_rss_peak,
            pg_cpu_pct,
            pg_mem_peak_mb: self.pg_mem_peak,
        }
    }
}

/// Parses `ps -o time=` output: `[[dd-]hh:]mm:ss[.ss]` (macOS and procps).
pub fn parse_ps_time(s: &str) -> Option<f64> {
    let s = s.trim();
    let (days, rest) = match s.split_once('-') {
        Some((d, r)) => (d.parse::<f64>().ok()?, r),
        None => (0.0, s),
    };
    let mut secs = 0.0;
    let mut mult = 1.0;
    for part in rest.rsplit(':') {
        secs += part.parse::<f64>().ok()? * mult;
        mult *= 60.0;
    }
    Some(days * 86_400.0 + secs)
}

async fn sample_ps(pids: [u32; 2]) -> [Option<ProcSample>; 2] {
    let mut out = [None, None];
    let Ok(o) = Command::new("ps")
        .args([
            "-o",
            "pid=,time=,rss=",
            "-p",
            &format!("{},{}", pids[0], pids[1]),
        ])
        .output()
        .await
    else {
        return out;
    };
    for line in String::from_utf8_lossy(&o.stdout).lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() != 3 {
            continue;
        }
        let (Ok(pid), Some(cpu_s), Ok(rss_kb)) = (
            f[0].parse::<u32>(),
            parse_ps_time(f[1]),
            f[2].parse::<f64>(),
        ) else {
            continue;
        };
        let s = ProcSample { cpu_s, rss_kb };
        if pid == pids[0] {
            out[0] = Some(s);
        } else if pid == pids[1] {
            out[1] = Some(s);
        }
    }
    out
}

/// Returns (cpu seconds, memory.current bytes) of the container's cgroup.
async fn sample_container(name: &str) -> Option<(f64, f64)> {
    let o = Command::new("docker")
        .args([
            "exec",
            name,
            "cat",
            "/sys/fs/cgroup/cpu.stat",
            "/sys/fs/cgroup/memory.current",
        ])
        .output()
        .await
        .ok()?;
    if !o.status.success() {
        return None;
    }
    parse_cgroup(&String::from_utf8_lossy(&o.stdout))
}

pub fn parse_cgroup(text: &str) -> Option<(f64, f64)> {
    let mut usage_usec = None;
    let mut mem = None;
    for line in text.lines() {
        let mut it = line.split_whitespace();
        match (it.next(), it.next()) {
            (Some("usage_usec"), Some(v)) => usage_usec = v.parse::<f64>().ok(),
            (Some(v), None) => mem = v.parse::<f64>().ok(),
            _ => {}
        }
    }
    Some((usage_usec? / 1e6, mem?))
}

pub struct ResourceSampler {
    stop: watch::Sender<bool>,
    handle: JoinHandle<Resources>,
}

impl ResourceSampler {
    pub fn start(server_pid: Option<u32>, container: Option<String>, at: Instant) -> Self {
        let (stop, mut rx) = watch::channel(false);
        let handle = tokio::spawn(async move {
            tokio::time::sleep_until(at.into()).await;
            let me = std::process::id();
            let server = server_pid.unwrap_or(0);
            let mut acc = ResAcc::default();
            loop {
                let t = Instant::now();
                let (ps, pg) = tokio::join!(sample_ps([server, me]), async {
                    match &container {
                        Some(c) => sample_container(c).await,
                        None => None,
                    }
                });
                acc.add(t, ps[0], ps[1], pg);
                if *rx.borrow() {
                    break;
                }
                tokio::select! {
                    _ = tokio::time::sleep(RESOURCE_SAMPLE_EVERY) => {}
                    _ = rx.changed() => {}
                }
            }
            acc.finish()
        });
        Self { stop, handle }
    }

    pub async fn stop(self) -> Resources {
        let _ = self.stop.send(true);
        self.handle.await.unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ps_time_formats() {
        assert_eq!(parse_ps_time("0:01.50"), Some(1.5));
        assert_eq!(parse_ps_time("00:01:02"), Some(62.0));
        assert_eq!(parse_ps_time("1:00:00.00"), Some(3600.0));
        assert_eq!(parse_ps_time("2-00:00:01"), Some(172_801.0));
        assert_eq!(parse_ps_time("x"), None);
    }

    #[test]
    fn cgroup_parse() {
        let t = "usage_usec 2500000\nuser_usec 1\nsystem_usec 2\n1048576\n";
        assert_eq!(parse_cgroup(t), Some((2.5, 1_048_576.0)));
        assert_eq!(parse_cgroup("usage_usec 1\n"), None);
    }

    #[test]
    fn activity_saturation_counts_full_pool() {
        let mut a = ActivityAcc::default();
        a.add((6, SERVER_POOL_MAX, 0, 2, 0, 1, 2));
        a.add((6, 1, 0, 0, 0, 0, 0));
        let r = a.finish();
        assert_eq!(r.samples, 2);
        assert_eq!(r.pool_all_busy_pct, 50.0);
        assert_eq!(r.advisory_wait_any_pct, 50.0);
        assert_eq!(r.advisory_wait_max, 2);
        assert_eq!(r.busy_max, SERVER_POOL_MAX);
    }
}
