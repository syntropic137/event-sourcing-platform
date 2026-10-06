//! Captures the environment a run was measured on, so results are only ever
//! compared like for like.

use std::collections::BTreeMap;

use serde::Serialize;
use sqlx::PgPool;
use tokio::process::Command;

/// Postgres settings that change durability or throughput materially.
const PG_SETTINGS: &[&str] = &[
    "server_version",
    "fsync",
    "synchronous_commit",
    "full_page_writes",
    "wal_sync_method",
    "wal_level",
    "wal_buffers",
    "commit_delay",
    "shared_buffers",
    "effective_cache_size",
    "work_mem",
    "max_connections",
    "max_wal_size",
    "checkpoint_timeout",
];

#[derive(Debug, Clone, Default, Serialize)]
pub struct Environment {
    pub git_sha: String,
    /// Uncommitted changes in the checkout: the SHA alone does not reproduce it.
    pub git_dirty: bool,
    pub os: String,
    pub host_cpu: String,
    pub host_logical_cpus: String,
    pub host_mem_gb: String,
    pub docker_server: String,
    pub docker_vm_cpus: String,
    pub docker_vm_mem_gb: String,
    pub pg_container: Option<String>,
    pub pg_image: String,
    pub pg_container_cpus: String,
    pub pg_container_mem_gb: String,
    pub pg_settings: BTreeMap<String, String>,
    pub server_pool_max: i64,
    pub rustc: String,
}

async fn sh(cmd: &str, args: &[&str]) -> String {
    match Command::new(cmd).args(args).output().await {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => "unknown".into(),
    }
}

fn gb(bytes: &str) -> String {
    bytes
        .trim()
        .parse::<f64>()
        .map(|b| format!("{:.1}", b / 1024.0 / 1024.0 / 1024.0))
        .unwrap_or_else(|_| "unknown".into())
}

pub async fn capture(pool: &PgPool, container: Option<&str>) -> Environment {
    let mut e = Environment {
        git_sha: sh("git", &["rev-parse", "HEAD"]).await,
        git_dirty: !sh("git", &["status", "--porcelain", "--untracked-files=no"])
            .await
            .is_empty(),
        os: sh("uname", &["-srm"]).await,
        rustc: sh("rustc", &["--version"]).await,
        server_pool_max: crate::sampler::SERVER_POOL_MAX,
        pg_container: container.map(str::to_owned),
        ..Default::default()
    };
    if cfg!(target_os = "macos") {
        e.host_cpu = sh("sysctl", &["-n", "machdep.cpu.brand_string"]).await;
        e.host_logical_cpus = sh("sysctl", &["-n", "hw.ncpu"]).await;
        e.host_mem_gb = gb(&sh("sysctl", &["-n", "hw.memsize"]).await);
    } else {
        e.host_cpu = sh(
            "sh",
            &["-c", "grep -m1 'model name' /proc/cpuinfo | cut -d: -f2"],
        )
        .await;
        e.host_logical_cpus = sh("nproc", &[]).await;
        e.host_mem_gb = sh(
            "sh",
            &[
                "-c",
                "awk '/MemTotal/ {printf \"%.1f\", $2/1048576}' /proc/meminfo",
            ],
        )
        .await;
    }
    let info = sh(
        "docker",
        &[
            "info",
            "--format",
            "{{.ServerVersion}} {{.NCPU}} {{.MemTotal}}",
        ],
    )
    .await;
    let mut it = info.split_whitespace();
    e.docker_server = it.next().unwrap_or("unknown").into();
    e.docker_vm_cpus = it.next().unwrap_or("unknown").into();
    e.docker_vm_mem_gb = gb(it.next().unwrap_or(""));
    if let Some(c) = container {
        let i = sh(
            "docker",
            &[
                "inspect",
                "--format",
                "{{.Config.Image}} {{.HostConfig.NanoCpus}} {{.HostConfig.Memory}}",
                c,
            ],
        )
        .await;
        let mut it = i.split_whitespace();
        e.pg_image = it.next().unwrap_or("unknown").into();
        e.pg_container_cpus = it
            .next()
            .and_then(|n| n.parse::<f64>().ok())
            .map(|n| {
                if n == 0.0 {
                    "unlimited".into()
                } else {
                    format!("{}", n / 1e9)
                }
            })
            .unwrap_or_else(|| "unknown".into());
        e.pg_container_mem_gb = match it.next() {
            Some("0") => "unlimited".into(),
            Some(m) => gb(m),
            None => "unknown".into(),
        };
    }
    for s in PG_SETTINGS {
        let v: Result<String, _> = sqlx::query_scalar(&format!("SHOW {s}"))
            .fetch_one(pool)
            .await;
        e.pg_settings
            .insert((*s).into(), v.unwrap_or_else(|_| "unknown".into()));
    }
    e
}
