//! `eventstore-bench`: live-Postgres baseline for the event store (#354).
//!
//! Usually run through `make bench-pg` / `scripts/bench-postgres.sh`, which
//! start a pinned Postgres container first. Direct use:
//!
//! ```text
//! eventstore-bench --database-url postgres://u:p@127.0.0.1:5432/db \
//!     --server-bin target/release/eventstore-bin \
//!     [--pg-container NAME] [--profile quick|full] [--durability LABEL] \
//!     [--out-dir DIR]
//! ```

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context};
use eventstore_bench::e2e::{run_e2e, E2eSpec};
use eventstore_bench::replay::{catchup, read_all_scan, read_stream_bench};
use eventstore_bench::report::{markdown, Results};
use eventstore_bench::workload::{preload, run_append, AppendSpec, Mode, PreloadSpec};
use eventstore_bench::{env, pb, Ctx};

struct Args {
    database_url: String,
    server_bin: PathBuf,
    pg_container: Option<String>,
    profile: String,
    durability: String,
    out_dir: Option<PathBuf>,
}

fn parse_args() -> anyhow::Result<Args> {
    let mut a = Args {
        database_url: std::env::var("DATABASE_URL").unwrap_or_default(),
        server_bin: PathBuf::from("target/release/eventstore-bin"),
        pg_container: None,
        profile: "quick".into(),
        durability: "unspecified".into(),
        out_dir: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        let mut v = || it.next().with_context(|| format!("{k} needs a value"));
        match k.as_str() {
            "--database-url" => a.database_url = v()?,
            "--server-bin" => a.server_bin = v()?.into(),
            "--pg-container" => a.pg_container = Some(v()?),
            "--profile" => a.profile = v()?,
            "--durability" => a.durability = v()?,
            "--out-dir" => a.out_dir = Some(v()?.into()),
            "-h" | "--help" => {
                println!("see docs/performance/POSTGRES-BASELINE.md");
                std::process::exit(0);
            }
            other => bail!("unknown argument {other}"),
        }
    }
    if a.database_url.is_empty() {
        bail!("--database-url (or DATABASE_URL) is required");
    }
    Ok(a)
}

struct Profile {
    warmup_s: f64,
    duration_s: f64,
    writers: Vec<usize>,
    ref_writers: usize,
    batches: Vec<usize>,
    payloads: Vec<usize>,
    open_fracs: Vec<f64>,
    open_lanes: usize,
    history: Vec<u64>,
    preload_writers: usize,
    readers: usize,
    readall_reps: usize,
    e2e_subs: Vec<usize>,
    e2e_rates: Vec<f64>,
    e2e_writers: usize,
    slow_rate: f64,
    slow_delay_ms: u64,
}

fn profile(name: &str) -> anyhow::Result<Profile> {
    Ok(match name {
        "quick" => Profile {
            warmup_s: 1.0,
            duration_s: 5.0,
            writers: vec![1, 8],
            ref_writers: 8,
            batches: vec![10, 100],
            payloads: vec![4096],
            open_fracs: vec![0.5],
            open_lanes: 32,
            history: vec![10_000],
            preload_writers: 16,
            readers: 8,
            readall_reps: 3,
            e2e_subs: vec![1, 8],
            e2e_rates: vec![250.0],
            e2e_writers: 32,
            slow_rate: 250.0,
            slow_delay_ms: 10,
        },
        "full" => Profile {
            warmup_s: 3.0,
            duration_s: 15.0,
            writers: vec![1, 4, 16, 32],
            ref_writers: 16,
            batches: vec![10, 100],
            payloads: vec![4096, 65536],
            open_fracs: vec![0.25, 0.5, 0.75, 0.9],
            open_lanes: 64,
            history: vec![10_000, 100_000, 1_000_000],
            preload_writers: 16,
            readers: 8,
            readall_reps: 3,
            e2e_subs: vec![1, 8, 32],
            e2e_rates: vec![250.0, 500.0],
            e2e_writers: 32,
            slow_rate: 250.0,
            slow_delay_ms: 10,
        },
        other => bail!("unknown profile {other} (quick|full)"),
    })
}

fn with_app_name(url: &str, app: &str) -> String {
    let sep = if url.contains('?') { '&' } else { '?' };
    format!("{url}{sep}application_name={app}")
}

async fn wait_ready(addr: &str, child: &mut tokio::process::Child) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(st) = child.try_wait()? {
            bail!("server exited during startup: {st}");
        }
        let probe = async {
            let mut c =
                pb::event_store_client::EventStoreClient::connect(format!("http://{addr}")).await?;
            c.read_all(pb::ReadAllRequest {
                tenant_id: "bench-ready-probe".into(),
                from_global_nonce: 0,
                max_count: 1,
                forward: true,
            })
            .await?;
            anyhow::Ok(())
        };
        if probe.await.is_ok() {
            return Ok(());
        }
        if Instant::now() > deadline {
            bail!("server not ready after 60s");
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

fn closed(
    name: String,
    writers: usize,
    same: bool,
    batch: usize,
    payload: usize,
    p: &Profile,
) -> AppendSpec {
    AppendSpec {
        name,
        writers,
        batch,
        payload,
        same_tenant: same,
        mode: Mode::Closed,
        warmup_s: p.warmup_s,
        duration_s: p.duration_s,
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = parse_args()?;
    let p = profile(&args.profile)?;
    let started = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let run_id = format!("r{started}");
    let out_dir = args
        .out_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("bench-results/{run_id}")));
    std::fs::create_dir_all(&out_dir)?;

    let server_app = format!("esp-bench-server-{run_id}");
    let addr = {
        let l = std::net::TcpListener::bind("127.0.0.1:0")?;
        format!("127.0.0.1:{}", l.local_addr()?.port())
    };
    let log = std::fs::File::create(out_dir.join("server.log"))?;
    let mut child = tokio::process::Command::new(&args.server_bin)
        .env("BACKEND", "postgres")
        .env(
            "DATABASE_URL",
            with_app_name(&args.database_url, &server_app),
        )
        .env("BIND_ADDR", &addr)
        .env_remove("RUST_LOG")
        .stdout(Stdio::null())
        .stderr(log)
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("spawning {}", args.server_bin.display()))?;
    wait_ready(&addr, &mut child).await?;

    let sampler_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&with_app_name(&args.database_url, "esp-bench-sampler"))
        .await?;
    let ctx = Ctx {
        addr,
        epoch: Instant::now(),
        run_id,
        server_app_name: server_app,
        sampler_pool,
        server_pid: child.id(),
        pg_container: args.pg_container.clone(),
    };
    let env = env::capture(&ctx.sampler_pool, args.pg_container.as_deref()).await;
    let t_run = Instant::now();
    let mut r = Results {
        profile: args.profile.clone(),
        durability: args.durability.clone(),
        started_unix_s: started,
        total_secs: 0.0,
        env,
        appends: vec![],
        preloads: vec![],
        read_all: vec![],
        read_stream: vec![],
        catchup: vec![],
        e2e: vec![],
    };

    // 1. Append: writers x tenant mode, batch size, payload size, open loop.
    let mut specs = Vec::new();
    for &w in &p.writers {
        for same in [true, false] {
            if w == 1 && !same {
                continue;
            }
            let t = if same { "same" } else { "diff" };
            specs.push(closed(
                format!("append-w{w}-{t}-b1-p256"),
                w,
                same,
                1,
                256,
                &p,
            ));
        }
    }
    let rw = p.ref_writers;
    if !p.writers.contains(&rw) {
        specs.push(closed(
            format!("append-w{rw}-same-b1-p256"),
            rw,
            true,
            1,
            256,
            &p,
        ));
    }
    for &b in &p.batches {
        specs.push(closed(
            format!("append-w{rw}-same-b{b}-p256"),
            rw,
            true,
            b,
            256,
            &p,
        ));
    }
    for &pl in &p.payloads {
        specs.push(closed(
            format!("append-w{rw}-same-b1-p{pl}"),
            rw,
            true,
            1,
            pl,
            &p,
        ));
    }
    for spec in specs {
        let a = run_append(&ctx, spec).await?;
        eprintln!(
            "[append] {}: {:.0} ev/s p99 {:.2} ms err {} verified {}",
            a.spec.name, a.events_per_sec, a.latency.p99_ms, a.errors, a.verify.ok
        );
        r.appends.push(a);
    }
    let ref_name = format!("append-w{rw}-same-b1-p256");
    let capacity_rps = r
        .appends
        .iter()
        .find(|a| a.spec.name == ref_name)
        .map(|a| a.requests_per_sec)
        .context("reference append scenario missing")?;
    for &f in &p.open_fracs {
        let spec = AppendSpec {
            name: format!("append-open-{:.0}pct-same-b1-p256", f * 100.0),
            writers: p.open_lanes,
            batch: 1,
            payload: 256,
            same_tenant: true,
            mode: Mode::Open {
                rps: (capacity_rps * f).max(1.0),
            },
            warmup_s: p.warmup_s,
            duration_s: p.duration_s,
        };
        let a = run_append(&ctx, spec).await?;
        eprintln!(
            "[append] {}: {:.0} ev/s p99 {:.2} ms (svc p99 {:.2}) drain {:.0} ms verified {}",
            a.spec.name,
            a.events_per_sec,
            a.latency.p99_ms,
            a.service.p99_ms,
            a.drain_ms,
            a.verify.ok
        );
        r.appends.push(a);
    }

    // 2. History: preload, replay paths, catch-up, append at that history.
    let hist_tenant = ctx.tenant("history");
    let mut acked_all: HashSet<String> = HashSet::new();
    let mut aggs: Vec<String> = Vec::new();
    let mut have = 0u64;
    for &h in &p.history {
        let (pre, writers) = preload(
            &ctx,
            PreloadSpec {
                tenant: &hist_tenant,
                tag: &format!("h{h}"),
                add: h - have,
                total_after: h,
                writers: p.preload_writers,
                batch: 100,
                payload: 256,
            },
        )
        .await?;
        have = h;
        for w in writers {
            acked_all.extend(w.acked);
            aggs.extend(w.completed);
        }
        eprintln!("[preload] {h}: {:.0} ev/s", pre.events_per_sec);
        r.preloads.push(pre);

        let (ra, seq) = read_all_scan(&ctx, &hist_tenant, &acked_all, p.readall_reps).await?;
        eprintln!(
            "[read_all] {h}: {:.0} ev/s verified {}",
            ra.events_per_sec_median, ra.verify.ok
        );
        r.read_all.push(ra);

        let rs = read_stream_bench(
            &ctx,
            &hist_tenant,
            h,
            Arc::new(aggs.clone()),
            p.readers,
            p.warmup_s,
            p.duration_s,
        )
        .await?;
        eprintln!(
            "[read_stream] {h}: {:.0} rehydrations/s p99 {:.2} ms",
            rs.rehydrations_per_sec, rs.latency.p99_ms
        );
        r.read_stream.push(rs);

        let cu = catchup(&ctx, &hist_tenant, &seq, Duration::from_secs(900)).await?;
        eprintln!(
            "[catchup] {h}: {:.0} ms, {:.0} ev/s exact {}",
            cu.caught_up_ms, cu.events_per_sec, cu.check.exact
        );
        r.catchup.push(cu);

        let a = run_append(
            &ctx,
            closed(
                format!("append-w{rw}-same-b1-p256-at-hist{h}"),
                rw,
                true,
                1,
                256,
                &p,
            ),
        )
        .await?;
        eprintln!(
            "[append] {}: {:.0} ev/s p99 {:.2} ms verified {}",
            a.spec.name, a.events_per_sec, a.latency.p99_ms, a.verify.ok
        );
        r.appends.push(a);
    }

    // 3. End-to-end delivery, then a slow subscriber next to fast ones.
    let mut e2e_specs = Vec::new();
    for &rate in &p.e2e_rates {
        for &s in &p.e2e_subs {
            e2e_specs.push(E2eSpec {
                name: format!("e2e-{rate:.0}eps-{s}subs"),
                fast_subscribers: s,
                slow_subscribers: 0,
                slow_delay_ms: 0,
                writers: p.e2e_writers,
                rate_eps: rate,
                payload: 256,
                warmup_s: p.warmup_s,
                duration_s: p.duration_s,
                drain_timeout_s: 120.0,
            });
        }
    }
    e2e_specs.push(E2eSpec {
        name: format!("e2e-{:.0}eps-slow", p.slow_rate),
        fast_subscribers: 2,
        slow_subscribers: 1,
        slow_delay_ms: p.slow_delay_ms,
        writers: p.e2e_writers,
        rate_eps: p.slow_rate,
        payload: 256,
        warmup_s: p.warmup_s,
        duration_s: p.duration_s,
        drain_timeout_s: 300.0,
    });
    for spec in e2e_specs {
        let e = run_e2e(&ctx, spec).await?;
        eprintln!(
            "[e2e] {}: deliver p99 {:.2} ms lag {}/{} drain {:.0}/{:.0} ms exact {}",
            e.spec.name,
            e.delivery.p99_ms,
            e.max_lag_fast,
            e.max_lag_slow,
            e.drain_ms_fast,
            e.drain_ms_slow,
            e.subscribers_exact && e.verify.ok
        );
        r.e2e.push(e);
    }

    r.total_secs = t_run.elapsed().as_secs_f64();
    let md = markdown(&r);
    std::fs::write(out_dir.join("results.json"), serde_json::to_vec_pretty(&r)?)?;
    std::fs::write(out_dir.join("summary.md"), &md)?;
    println!("{md}");
    eprintln!("results: {}", out_dir.display());
    let _ = child.kill().await;
    if !r.all_verified() {
        bail!("completeness/ordering verification FAILED; see summary");
    }
    Ok(())
}
