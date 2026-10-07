//! Disposable Postgres containers driven through the docker CLI.
//!
//! Each container is created by the drill, labelled
//! `esp-recovery-drill=<DRILL_RUN_ID>`, and removed (with its volumes) on
//! drop, including when a drill panics. `make recovery-drill` also removes any
//! container left with its run label if the test process itself is killed.

use std::net::SocketAddr;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use super::{eventually, free_port, unique};

pub const LABEL_KEY: &str = "esp-recovery-drill";
pub const DB: &str = "drill";
pub const USER: &str = "drill";
const PASSWORD: &str = "drill";

/// Durability settings the drills run under and assert. These are the
/// Postgres defaults; they are passed explicitly so a changed image default
/// cannot silently weaken the drill.
pub const DURABILITY_SETTINGS: &[(&str, &str)] = &[
    ("fsync", "on"),
    ("synchronous_commit", "on"),
    ("full_page_writes", "on"),
];

pub fn run_label() -> String {
    std::env::var("DRILL_RUN_ID").unwrap_or_else(|_| "adhoc".into())
}

pub fn image() -> String {
    std::env::var("DRILL_PG_IMAGE").unwrap_or_else(|_| "postgres:15".into())
}

pub fn docker(args: &[&str]) -> Output {
    Command::new("docker")
        .args(args)
        .output()
        .expect("the docker CLI is required for recovery drills")
}

/// Run a docker command that must succeed; returns trimmed stdout.
pub fn docker_ok(args: &[&str]) -> String {
    let out = docker(args);
    assert!(
        out.status.success(),
        "docker {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

pub struct DisposablePg {
    pub name: String,
    pub port: u16,
}

impl DisposablePg {
    /// Create and start a fresh, empty Postgres container.
    pub async fn start(role: &str) -> Self {
        let port = free_port();
        let name = unique(&format!("esp-drill-{role}"));
        let label = format!("{LABEL_KEY}={}", run_label());
        let publish = format!("127.0.0.1:{port}:5432");
        let image = image();
        let mut args = vec![
            "run".to_owned(),
            "-d".into(),
            "--name".into(),
            name.clone(),
            "--label".into(),
            label,
            "-e".into(),
            format!("POSTGRES_USER={USER}"),
            "-e".into(),
            format!("POSTGRES_PASSWORD={PASSWORD}"),
            "-e".into(),
            format!("POSTGRES_DB={DB}"),
            "-p".into(),
            publish,
            image,
        ];
        for (k, v) in DURABILITY_SETTINGS {
            args.push("-c".into());
            args.push(format!("{k}={v}"));
        }
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        docker_ok(&refs);
        let pg = Self { name, port };
        pg.wait_ready().await;
        pg
    }

    pub fn url(&self) -> String {
        self.url_for(DB)
    }

    /// URL of another database in this container.
    pub fn url_for(&self, db: &str) -> String {
        format!("postgres://{USER}:{PASSWORD}@127.0.0.1:{}/{db}", self.port)
    }

    pub fn addr(&self) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], self.port))
    }

    /// Wait until the final (post-initdb) server accepts TCP connections.
    /// The image's temporary init server listens on a Unix socket only, so a
    /// successful TCP query means the real server is up.
    pub async fn wait_ready(&self) {
        let url = self.url();
        eventually(
            &format!("postgres {} to accept connections", self.name),
            Duration::from_secs(120),
            || {
                let url = url.clone();
                async move {
                    let pool = PgPoolOptions::new()
                        .max_connections(1)
                        .acquire_timeout(Duration::from_secs(2))
                        .connect(&url)
                        .await
                        .ok()?;
                    let ok = sqlx::query("SELECT 1").execute(&pool).await.is_ok();
                    pool.close().await;
                    ok.then_some(())
                }
            },
        )
        .await;
    }

    /// A direct (non-proxied) pool for drill assertions and side sessions.
    pub async fn pool(&self) -> PgPool {
        PgPoolOptions::new()
            .max_connections(6)
            .acquire_timeout(Duration::from_secs(30))
            .connect(&self.url())
            .await
            .expect("connect drill pool")
    }

    /// SIGKILL the postgres server (crash, not a clean shutdown). The OS page
    /// cache survives, so this is not a power-loss test.
    pub fn crash(&self) {
        docker_ok(&["kill", "--signal", "KILL", &self.name]);
    }

    /// Start the same container (same data directory) again and wait for it.
    pub async fn start_again(&self) {
        docker_ok(&["start", &self.name]);
        self.wait_ready().await;
    }

    pub fn exec(&self, args: &[&str]) -> String {
        let mut full = vec!["exec", self.name.as_str()];
        full.extend_from_slice(args);
        docker_ok(&full)
    }

    pub fn copy_out(&self, container_path: &str, host_path: &Path) {
        let src = format!("{}:{container_path}", self.name);
        docker_ok(&["cp", &src, host_path.to_str().unwrap()]);
    }

    pub fn copy_in(&self, host_path: &Path, container_path: &str) {
        let dst = format!("{}:{container_path}", self.name);
        docker_ok(&["cp", host_path.to_str().unwrap(), &dst]);
    }

    /// Server version string, e.g. "15.8 (Debian 15.8-1.pgdg120+1)".
    pub fn server_version(&self) -> String {
        self.exec(&["psql", "-U", USER, "-d", DB, "-Atc", "SHOW server_version"])
    }
}

impl Drop for DisposablePg {
    fn drop(&mut self) {
        if std::env::var_os("DRILL_KEEP_CONTAINERS").is_some() {
            eprintln!("DRILL_KEEP_CONTAINERS set: leaving container {}", self.name);
            return;
        }
        let _ = docker(&["rm", "-f", "-v", &self.name]);
    }
}

/// Assert the durability configuration the drill claims to run under.
pub async fn assert_durability_settings(pool: &PgPool) {
    for (name, expected) in DURABILITY_SETTINGS {
        let actual: String = sqlx::query_scalar(&format!("SHOW {name}"))
            .fetch_one(pool)
            .await
            .expect("SHOW setting");
        assert_eq!(&actual, expected, "postgres setting {name}");
    }
}
