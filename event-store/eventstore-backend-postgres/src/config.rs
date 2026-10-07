//! Connection pool and timeout configuration for the Postgres backend
//! (#368, #370). See docs/operations/POSTGRES-CONNECTIONS.md.
//!
//! Two layers bound every database call:
//!
//! - **Server side** (session GUCs sent at connect): `statement_timeout`,
//!   `lock_timeout`, `idle_in_transaction_session_timeout` and TCP
//!   keepalives. These free server resources, above all the per-tenant
//!   append-order advisory lock, when a client stalls or vanishes.
//! - **Client side**: a deadline per operation (`statement_timeout` plus
//!   [`CLIENT_DEADLINE_GRACE`]) and the pool acquire timeout. These are what
//!   bound a call when the network silently drops packets: the server cannot
//!   report a timeout over a dead path, so the client must stop waiting.

use std::time::Duration;

use sqlx::postgres::PgConnectOptions;

/// Added to `statement_timeout` to form the client-side deadline, so the
/// server's own timeout (a clean, attributable error) normally fires first.
pub const CLIENT_DEADLINE_GRACE: Duration = Duration::from_secs(5);

/// Pool and timeout settings. `None` disables a timeout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostgresConfig {
    /// Most pooled connections (`PG_POOL_MAX_CONNECTIONS`).
    pub max_connections: u32,
    /// Connections kept open while idle (`PG_POOL_MIN_CONNECTIONS`).
    pub min_connections: u32,
    /// Longest wait for a pooled connection, including connecting
    /// (`PG_ACQUIRE_TIMEOUT_MS`).
    pub acquire_timeout: Duration,
    /// Server `statement_timeout`; also sets the client deadline
    /// (`PG_STATEMENT_TIMEOUT_MS`).
    pub statement_timeout: Option<Duration>,
    /// Server `lock_timeout`: longest wait for a row or advisory lock
    /// (`PG_LOCK_TIMEOUT_MS`).
    pub lock_timeout: Option<Duration>,
    /// Server `idle_in_transaction_session_timeout` (`PG_IDLE_IN_TRANSACTION_TIMEOUT_MS`).
    pub idle_in_transaction_timeout: Option<Duration>,
    /// Server `tcp_keepalives_idle`; interval is a third of it, 3 probes
    /// (`PG_TCP_KEEPALIVE_SECS`).
    pub tcp_keepalive: Option<Duration>,
}

impl Default for PostgresConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
            min_connections: 0,
            acquire_timeout: Duration::from_secs(30),
            statement_timeout: Some(Duration::from_secs(30)),
            lock_timeout: Some(Duration::from_secs(10)),
            idle_in_transaction_timeout: Some(Duration::from_secs(10)),
            tcp_keepalive: Some(Duration::from_secs(30)),
        }
    }
}

pub const ENV_MAX_CONNECTIONS: &str = "PG_POOL_MAX_CONNECTIONS";
pub const ENV_MIN_CONNECTIONS: &str = "PG_POOL_MIN_CONNECTIONS";
pub const ENV_ACQUIRE_TIMEOUT_MS: &str = "PG_ACQUIRE_TIMEOUT_MS";
pub const ENV_STATEMENT_TIMEOUT_MS: &str = "PG_STATEMENT_TIMEOUT_MS";
pub const ENV_LOCK_TIMEOUT_MS: &str = "PG_LOCK_TIMEOUT_MS";
pub const ENV_IDLE_IN_TRANSACTION_TIMEOUT_MS: &str = "PG_IDLE_IN_TRANSACTION_TIMEOUT_MS";
pub const ENV_TCP_KEEPALIVE_SECS: &str = "PG_TCP_KEEPALIVE_SECS";

impl PostgresConfig {
    /// Read settings from the process environment; unset keys keep their
    /// defaults. Invalid values are errors, never silently defaulted.
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    /// [`Self::from_env`] with an injectable lookup (tests).
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> anyhow::Result<Self> {
        let d = Self::default();
        let num = |key: &str| -> anyhow::Result<Option<u64>> {
            match get(key) {
                None => Ok(None),
                Some(v) if v.trim().is_empty() => Ok(None),
                Some(v) => v.trim().parse::<u64>().map(Some).map_err(|_| {
                    anyhow::anyhow!("{key} must be a non-negative integer, got '{v}'")
                }),
            }
        };
        // 0 disables an optional timeout.
        let opt = |key: &str, unit: fn(u64) -> Duration, default: Option<Duration>| {
            Ok::<_, anyhow::Error>(match num(key)? {
                None => default,
                Some(0) => None,
                Some(n) => Some(unit(n)),
            })
        };
        let cfg = Self {
            max_connections: match num(ENV_MAX_CONNECTIONS)? {
                None => d.max_connections,
                Some(n) => u32::try_from(n)
                    .map_err(|_| anyhow::anyhow!("{ENV_MAX_CONNECTIONS} is too large"))?,
            },
            min_connections: match num(ENV_MIN_CONNECTIONS)? {
                None => d.min_connections,
                Some(n) => u32::try_from(n)
                    .map_err(|_| anyhow::anyhow!("{ENV_MIN_CONNECTIONS} is too large"))?,
            },
            acquire_timeout: match num(ENV_ACQUIRE_TIMEOUT_MS)? {
                None => d.acquire_timeout,
                Some(n) => Duration::from_millis(n),
            },
            statement_timeout: opt(
                ENV_STATEMENT_TIMEOUT_MS,
                Duration::from_millis,
                d.statement_timeout,
            )?,
            lock_timeout: opt(ENV_LOCK_TIMEOUT_MS, Duration::from_millis, d.lock_timeout)?,
            idle_in_transaction_timeout: opt(
                ENV_IDLE_IN_TRANSACTION_TIMEOUT_MS,
                Duration::from_millis,
                d.idle_in_transaction_timeout,
            )?,
            tcp_keepalive: opt(ENV_TCP_KEEPALIVE_SECS, Duration::from_secs, d.tcp_keepalive)?,
        };
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.max_connections >= 1,
            "{ENV_MAX_CONNECTIONS} must be at least 1"
        );
        anyhow::ensure!(
            self.min_connections <= self.max_connections,
            "{ENV_MIN_CONNECTIONS} ({}) must not exceed {ENV_MAX_CONNECTIONS} ({})",
            self.min_connections,
            self.max_connections
        );
        anyhow::ensure!(
            !self.acquire_timeout.is_zero(),
            "{ENV_ACQUIRE_TIMEOUT_MS} must be at least 1"
        );
        // Postgres takes these GUCs as int milliseconds.
        for (key, v) in [
            (ENV_STATEMENT_TIMEOUT_MS, self.statement_timeout),
            (ENV_LOCK_TIMEOUT_MS, self.lock_timeout),
            (
                ENV_IDLE_IN_TRANSACTION_TIMEOUT_MS,
                self.idle_in_transaction_timeout,
            ),
        ] {
            if let Some(v) = v {
                anyhow::ensure!(
                    v.as_millis() <= i32::MAX as u128,
                    "{key} must be at most {} ms",
                    i32::MAX
                );
            }
        }
        if let Some(k) = self.tcp_keepalive {
            anyhow::ensure!(
                k.as_secs() <= i32::MAX as u64,
                "{ENV_TCP_KEEPALIVE_SECS} is too large"
            );
        }
        Ok(())
    }

    /// Client-side bound on one operation: `statement_timeout` plus
    /// [`CLIENT_DEADLINE_GRACE`], or `None` when the statement timeout is
    /// disabled.
    pub fn operation_deadline(&self) -> Option<Duration> {
        self.statement_timeout.map(|t| t + CLIENT_DEADLINE_GRACE)
    }

    /// Session settings sent in the startup packet (`options=-c k=v`).
    pub fn session_settings(&self) -> Vec<(&'static str, String)> {
        let ms = |d: Option<Duration>| d.map(|d| d.as_millis()).unwrap_or(0).to_string();
        let mut s = vec![
            ("statement_timeout", ms(self.statement_timeout)),
            ("lock_timeout", ms(self.lock_timeout)),
            (
                "idle_in_transaction_session_timeout",
                ms(self.idle_in_transaction_timeout),
            ),
        ];
        // Ignored by Postgres on Unix-socket connections. Left at the
        // server's default when disabled.
        if let Some(k) = self.tcp_keepalive {
            let idle = k.as_secs().max(1);
            s.push(("tcp_keepalives_idle", idle.to_string()));
            s.push(("tcp_keepalives_interval", (idle / 3).max(1).to_string()));
            s.push(("tcp_keepalives_count", "3".to_string()));
        }
        s
    }

    /// `base` with this config's session settings added.
    pub fn apply(&self, base: PgConnectOptions) -> PgConnectOptions {
        base.options(self.session_settings())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn parse(pairs: &[(&str, &str)]) -> anyhow::Result<PostgresConfig> {
        let m: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        PostgresConfig::from_lookup(|k| m.get(k).cloned())
    }

    #[test]
    fn unset_env_yields_documented_defaults() {
        let c = parse(&[]).unwrap();
        assert_eq!(c, PostgresConfig::default());
        assert_eq!(c.max_connections, 10);
        assert_eq!(c.min_connections, 0);
        assert_eq!(c.acquire_timeout, Duration::from_secs(30));
        assert_eq!(c.statement_timeout, Some(Duration::from_secs(30)));
        assert_eq!(c.lock_timeout, Some(Duration::from_secs(10)));
        assert_eq!(c.idle_in_transaction_timeout, Some(Duration::from_secs(10)));
        assert_eq!(c.tcp_keepalive, Some(Duration::from_secs(30)));
        assert_eq!(c.operation_deadline(), Some(Duration::from_secs(35)));
    }

    #[test]
    fn env_overrides_and_zero_disables() {
        let c = parse(&[
            (ENV_MAX_CONNECTIONS, "32"),
            (ENV_MIN_CONNECTIONS, "4"),
            (ENV_ACQUIRE_TIMEOUT_MS, "1500"),
            (ENV_STATEMENT_TIMEOUT_MS, "0"),
            (ENV_LOCK_TIMEOUT_MS, "250"),
            (ENV_IDLE_IN_TRANSACTION_TIMEOUT_MS, " 2000 "),
            (ENV_TCP_KEEPALIVE_SECS, "0"),
        ])
        .unwrap();
        assert_eq!(c.max_connections, 32);
        assert_eq!(c.min_connections, 4);
        assert_eq!(c.acquire_timeout, Duration::from_millis(1500));
        assert_eq!(c.statement_timeout, None);
        assert_eq!(c.operation_deadline(), None);
        assert_eq!(c.lock_timeout, Some(Duration::from_millis(250)));
        assert_eq!(c.idle_in_transaction_timeout, Some(Duration::from_secs(2)));
        assert_eq!(c.tcp_keepalive, None);
    }

    #[test]
    fn invalid_values_are_rejected_not_defaulted() {
        for (k, v) in [
            (ENV_MAX_CONNECTIONS, "ten"),
            (ENV_MAX_CONNECTIONS, "-1"),
            (ENV_MAX_CONNECTIONS, "0"),
            (ENV_ACQUIRE_TIMEOUT_MS, "0"),
            (ENV_STATEMENT_TIMEOUT_MS, "1.5"),
            (ENV_STATEMENT_TIMEOUT_MS, "99999999999"),
            (ENV_MAX_CONNECTIONS, "99999999999"),
        ] {
            let err = parse(&[(k, v)]).expect_err(&format!("{k}={v} must be rejected"));
            assert!(format!("{err:#}").contains(k), "{err:#}");
        }
        let err = parse(&[(ENV_MAX_CONNECTIONS, "2"), (ENV_MIN_CONNECTIONS, "3")]).unwrap_err();
        assert!(format!("{err:#}").contains(ENV_MIN_CONNECTIONS), "{err:#}");
    }

    #[test]
    fn session_settings_carry_timeouts_and_keepalives() {
        let s: HashMap<_, _> = PostgresConfig::default()
            .session_settings()
            .into_iter()
            .collect();
        assert_eq!(s["statement_timeout"], "30000");
        assert_eq!(s["lock_timeout"], "10000");
        assert_eq!(s["idle_in_transaction_session_timeout"], "10000");
        assert_eq!(s["tcp_keepalives_idle"], "30");
        assert_eq!(s["tcp_keepalives_interval"], "10");
        assert_eq!(s["tcp_keepalives_count"], "3");

        let off = PostgresConfig {
            statement_timeout: None,
            tcp_keepalive: None,
            ..Default::default()
        };
        let s: HashMap<_, _> = off.session_settings().into_iter().collect();
        assert_eq!(s["statement_timeout"], "0", "disabled is sent as 0");
        assert!(!s.contains_key("tcp_keepalives_idle"));
    }
}
