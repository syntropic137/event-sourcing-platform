//! A representative consumer: an account-balance projection fed by gRPC
//! `Subscribe`, with consumer-owned upcasters and a persisted checkpoint.
//!
//! State, dedupe ledger and checkpoint live in the event store database
//! (`projection_checkpoints` from the store's migrations, plus `drill_*`
//! tables), so a backup carries them together. Each event is applied in one
//! transaction: dedupe on `event_id`, update state, and (every
//! `checkpoint_every` events) save the checkpoint. A redelivered event is
//! detected and skipped, so at-least-once delivery yields an idempotent
//! result.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use eventstore_proto::gen::{EventData, SubscribeRequest};
use sqlx::{PgPool, Row};
use tonic::Status;

use super::server::connect;
use super::STEP;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS drill_balances (
    projection     TEXT NOT NULL,
    tenant_id      TEXT NOT NULL,
    account_id     TEXT NOT NULL,
    owner          TEXT NOT NULL,
    currency       TEXT NOT NULL,
    balance_minor  BIGINT NOT NULL,
    events_applied BIGINT NOT NULL,
    PRIMARY KEY (projection, tenant_id, account_id)
);
CREATE TABLE IF NOT EXISTS drill_applied (
    projection   TEXT NOT NULL,
    tenant_id    TEXT NOT NULL,
    event_id     TEXT NOT NULL,
    global_nonce BIGINT NOT NULL,
    PRIMARY KEY (projection, tenant_id, event_id)
);
"#;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountState {
    pub owner: String,
    pub currency: String,
    pub balance_minor: i64,
    pub events_applied: i64,
}

/// Domain event in its current shape (after upcasting).
#[derive(Debug, PartialEq)]
pub enum DomainEvent {
    Opened { owner: String, currency: String },
    Deposited { amount_minor: i64 },
    Withdrawn { amount_minor: i64 },
}

/// Consumer-owned upcaster registry. The store keeps `event_type`,
/// `event_version` and payload bytes verbatim forever; only the consumer can
/// interpret old versions, so it must retain these upcasters for replay.
#[derive(Clone, Debug)]
pub struct Upcasters {
    known: BTreeSet<(String, u32)>,
}

impl Upcasters {
    pub fn full() -> Self {
        let known = [
            ("AccountOpened", 1),
            ("AccountOpened", 2),
            ("FundsDeposited", 1),
            ("FundsDeposited", 2),
            ("FundsWithdrawn", 1),
        ]
        .into_iter()
        .map(|(t, v)| (t.to_owned(), v))
        .collect();
        Self { known }
    }

    /// The registry with one historical version dropped, as if a consumer
    /// deleted an "old" upcaster.
    pub fn without(mut self, event_type: &str, version: u32) -> Self {
        self.known.remove(&(event_type.to_owned(), version));
        self
    }

    pub fn decode(&self, ev: &EventData) -> Result<DomainEvent, String> {
        let meta = ev.meta.as_ref().ok_or("event without metadata")?;
        let key = (meta.event_type.clone(), meta.event_version);
        if !self.known.contains(&key) {
            return Err(format!(
                "no upcaster for {} v{} at global_nonce {}",
                meta.event_type, meta.event_version, meta.global_nonce
            ));
        }
        let v: serde_json::Value = serde_json::from_slice(&ev.payload)
            .map_err(|e| format!("payload at global_nonce {}: {e}", meta.global_nonce))?;
        let s = |k: &str| v[k].as_str().map(str::to_owned);
        let i = |k: &str| v[k].as_i64();
        let bad = || format!("malformed {} v{}", meta.event_type, meta.event_version);
        Ok(match (meta.event_type.as_str(), meta.event_version) {
            // v1 had no currency: every v1 account was USD.
            ("AccountOpened", 1) => DomainEvent::Opened {
                owner: s("owner").ok_or_else(bad)?,
                currency: "USD".into(),
            },
            ("AccountOpened", 2) => DomainEvent::Opened {
                owner: s("owner").ok_or_else(bad)?,
                currency: s("currency").ok_or_else(bad)?,
            },
            // v1 amounts were whole currency units.
            ("FundsDeposited", 1) => DomainEvent::Deposited {
                amount_minor: i("amount").ok_or_else(bad)? * 100,
            },
            ("FundsDeposited", 2) => DomainEvent::Deposited {
                amount_minor: i("amount_minor").ok_or_else(bad)?,
            },
            ("FundsWithdrawn", 1) => DomainEvent::Withdrawn {
                amount_minor: i("amount_minor").ok_or_else(bad)?,
            },
            _ => unreachable!("registry and match arms are in sync"),
        })
    }
}

/// When a consumer run stops on its own.
#[derive(Clone, Copy, Debug)]
pub enum Stop {
    /// Once the checkpoint reaches this global nonce.
    AtGlobalNonce(u64),
    /// After applying this many events in this run (simulated crash: the
    /// stream is dropped without any cleanup).
    AfterApplied(usize),
    /// Only on an error or end of stream.
    Never,
}

#[derive(Debug)]
pub enum Outcome {
    Reached,
    Crashed,
    /// The subscription failed with this status.
    Failed(Status),
    /// The stream ended without an error status.
    EndedWithoutError,
    /// A consumer handler (upcaster) rejected an event; nothing after it was
    /// applied and the checkpoint did not move past it.
    HandlerFailed(String),
}

#[derive(Debug)]
pub struct RunReport {
    pub from: u64,
    pub delivered: usize,
    pub applied: usize,
    pub duplicates_skipped: usize,
    pub caught_up_markers: usize,
    pub outcome: Outcome,
}

impl RunReport {
    pub fn failed_status(&self) -> &Status {
        match &self.outcome {
            Outcome::Failed(s) => s,
            other => panic!("expected a failed subscription, got {other:?} ({self:?})"),
        }
    }
}

pub struct Consumer {
    pub pool: PgPool,
    pub projection: String,
    pub tenant: String,
    pub upcasters: Upcasters,
    pub checkpoint_every: u64,
}

impl Consumer {
    pub async fn new(pool: PgPool, projection: &str, tenant: &str) -> Self {
        sqlx::raw_sql(SCHEMA).execute(&pool).await.expect("schema");
        Self {
            pool,
            projection: projection.into(),
            tenant: tenant.into(),
            upcasters: Upcasters::full(),
            checkpoint_every: 1,
        }
    }

    fn checkpoint_name(&self) -> String {
        format!("{}:{}", self.projection, self.tenant)
    }

    pub async fn checkpoint(&self) -> u64 {
        checkpoint_of(&self.pool, &self.checkpoint_name()).await
    }

    /// Subscribe from `checkpoint + 1` and apply events until `stop`.
    pub async fn run(&self, endpoint: &str, stop: Stop) -> RunReport {
        let from = self.checkpoint().await + 1;
        let mut report = RunReport {
            from,
            delivered: 0,
            applied: 0,
            duplicates_skipped: 0,
            caught_up_markers: 0,
            outcome: Outcome::EndedWithoutError,
        };
        let mut client = connect(endpoint).await;
        let call = client.subscribe(SubscribeRequest {
            tenant_id: self.tenant.clone(),
            aggregate_id_prefix: String::new(),
            from_global_nonce: from,
        });
        let mut stream = match tokio::time::timeout(STEP, call).await {
            Ok(Ok(resp)) => resp.into_inner(),
            Ok(Err(status)) => {
                report.outcome = Outcome::Failed(status);
                return report;
            }
            Err(_) => panic!("subscribe call hung for {STEP:?}"),
        };
        let mut since_checkpoint = 0u64;
        // Highest position this consumer has handled (applied or skipped).
        let mut progress = from - 1;
        loop {
            if let Stop::AtGlobalNonce(target) = stop {
                if progress >= target {
                    report.outcome = Outcome::Reached;
                    return report;
                }
            }
            let msg = match tokio::time::timeout(STEP, stream.message()).await {
                Ok(m) => m,
                Err(_) => {
                    panic!("subscription silent for {STEP:?} ({report:?}); a hidden failure?")
                }
            };
            let ev = match msg {
                Ok(Some(resp)) => match resp.event {
                    Some(ev) => ev,
                    None => {
                        report.caught_up_markers += 1;
                        continue;
                    }
                },
                Ok(None) => {
                    report.outcome = Outcome::EndedWithoutError;
                    return report;
                }
                Err(status) => {
                    report.outcome = Outcome::Failed(status);
                    return report;
                }
            };
            report.delivered += 1;
            let g = ev.meta.as_ref().map(|m| m.global_nonce).unwrap_or(0);
            let save = since_checkpoint + 1 >= self.checkpoint_every;
            match self.apply(&ev, save).await {
                Ok(true) => {
                    report.applied += 1;
                    progress = progress.max(g);
                    since_checkpoint = if save { 0 } else { since_checkpoint + 1 };
                }
                Ok(false) => {
                    report.duplicates_skipped += 1;
                    progress = progress.max(g);
                }
                Err(e) => {
                    report.outcome = Outcome::HandlerFailed(e);
                    return report;
                }
            }
            if let Stop::AfterApplied(n) = stop {
                if report.applied >= n {
                    report.outcome = Outcome::Crashed;
                    return report;
                }
            }
        }
    }

    /// Apply one event atomically. Returns false for a redelivered event.
    async fn apply(&self, ev: &EventData, save_checkpoint: bool) -> Result<bool, String> {
        let meta = ev.meta.as_ref().expect("meta");
        let domain = self.upcasters.decode(ev)?;
        let mut tx = self.pool.begin().await.expect("begin");
        let inserted = sqlx::query(
            "INSERT INTO drill_applied (projection, tenant_id, event_id, global_nonce)
             VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
        )
        .bind(&self.projection)
        .bind(&self.tenant)
        .bind(&meta.event_id)
        .bind(meta.global_nonce as i64)
        .execute(&mut *tx)
        .await
        .expect("dedupe insert")
        .rows_affected();
        if inserted == 0 {
            tx.rollback().await.expect("rollback");
            return Ok(false);
        }
        let account = &meta.aggregate_id;
        match domain {
            DomainEvent::Opened { owner, currency } => {
                sqlx::query("INSERT INTO drill_balances VALUES ($1, $2, $3, $4, $5, 0, 1)")
                    .bind(&self.projection)
                    .bind(&self.tenant)
                    .bind(account)
                    .bind(owner)
                    .bind(currency)
                    .execute(&mut *tx)
                    .await
                    .expect("open account");
            }
            DomainEvent::Deposited { amount_minor } => {
                self.adjust(&mut tx, account, amount_minor).await;
            }
            DomainEvent::Withdrawn { amount_minor } => {
                self.adjust(&mut tx, account, -amount_minor).await;
            }
        }
        if save_checkpoint {
            sqlx::query(
                "INSERT INTO projection_checkpoints (projection_name, global_position)
                 VALUES ($1, $2)
                 ON CONFLICT (projection_name)
                 DO UPDATE SET global_position = EXCLUDED.global_position, updated_at = NOW()",
            )
            .bind(self.checkpoint_name())
            .bind(meta.global_nonce as i64)
            .execute(&mut *tx)
            .await
            .expect("save checkpoint");
        }
        tx.commit().await.expect("commit");
        Ok(true)
    }

    async fn adjust(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        account: &str,
        delta: i64,
    ) {
        let n = sqlx::query(
            "UPDATE drill_balances
                SET balance_minor = balance_minor + $4, events_applied = events_applied + 1
              WHERE projection = $1 AND tenant_id = $2 AND account_id = $3",
        )
        .bind(&self.projection)
        .bind(&self.tenant)
        .bind(account)
        .bind(delta)
        .execute(&mut **tx)
        .await
        .expect("adjust balance")
        .rows_affected();
        assert_eq!(n, 1, "event for unopened account {account}");
    }

    pub async fn state(&self) -> BTreeMap<String, AccountState> {
        state_of(&self.pool, &self.projection, &self.tenant).await
    }

    /// Number of distinct events applied (each at most once by construction).
    pub async fn applied_count(&self) -> i64 {
        sqlx::query_scalar(
            "SELECT count(*) FROM drill_applied WHERE projection = $1 AND tenant_id = $2",
        )
        .bind(&self.projection)
        .bind(&self.tenant)
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }

    /// Assert the projection covers exactly the tenant's log: every stored
    /// event applied once, nothing else.
    pub async fn assert_complete(&self) {
        let missing: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM events e
              WHERE e.tenant_id = $2
                AND NOT EXISTS (SELECT 1 FROM drill_applied a
                                 WHERE a.projection = $1 AND a.tenant_id = e.tenant_id
                                   AND a.event_id = e.event_id
                                   AND a.global_nonce = e.global_nonce)",
        )
        .bind(&self.projection)
        .bind(&self.tenant)
        .fetch_one(&self.pool)
        .await
        .unwrap();
        assert_eq!(missing, 0, "events not applied by {}", self.projection);
        let total: i64 = sqlx::query_scalar("SELECT count(*) FROM events WHERE tenant_id = $1")
            .bind(&self.tenant)
            .fetch_one(&self.pool)
            .await
            .unwrap();
        assert_eq!(self.applied_count().await, total, "applied != stored");
    }

    /// Run until the checkpoint reaches the tenant's head, retrying with a
    /// fresh subscription from the checkpoint after any failure.
    pub async fn run_to(&self, endpoint: &str, target: u64) -> Vec<RunReport> {
        let mut reports = Vec::new();
        for _ in 0..50 {
            let r = self.run(endpoint, Stop::AtGlobalNonce(target)).await;
            let done = matches!(r.outcome, Outcome::Reached);
            if let Outcome::HandlerFailed(e) = &r.outcome {
                panic!("handler failed: {e}");
            }
            reports.push(r);
            if done {
                return reports;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("consumer never reached {target}: {reports:?}");
    }
}

pub async fn checkpoint_of(pool: &PgPool, name: &str) -> u64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT global_position FROM projection_checkpoints WHERE projection_name = $1",
    )
    .bind(name)
    .fetch_optional(pool)
    .await
    .unwrap()
    .unwrap_or(0) as u64
}

pub async fn state_of(
    pool: &PgPool,
    projection: &str,
    tenant: &str,
) -> BTreeMap<String, AccountState> {
    sqlx::query(
        "SELECT account_id, owner, currency, balance_minor, events_applied
           FROM drill_balances WHERE projection = $1 AND tenant_id = $2",
    )
    .bind(projection)
    .bind(tenant)
    .fetch_all(pool)
    .await
    .unwrap()
    .into_iter()
    .map(|r| {
        (
            r.get("account_id"),
            AccountState {
                owner: r.get("owner"),
                currency: r.get("currency"),
                balance_minor: r.get("balance_minor"),
                events_applied: r.get("events_applied"),
            },
        )
    })
    .collect()
}
