//! Supervision for [`ProjectionRunner`]: reconnect with backoff, halt on
//! undecodable events (ADR-026), health reporting.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::{CheckpointedProjection, ProjectionRunner, ProjectionStore, RunExit};
use crate::error::{Error, Result};

/// Jittered exponential backoff for reconnecting a supervised runner.
///
/// The delay before retry `n` (1-based) is
/// `base = min(initial * multiplier^(n-1), max)`, reduced by a random
/// fraction of up to `jitter` of itself: it lies in
/// `[base * (1 - jitter), base]` and never exceeds `max`. Jitter spreads the
/// reconnects of many runners after a shared outage.
#[derive(Debug, Clone, PartialEq)]
pub struct BackoffPolicy {
    initial: Duration,
    max: Duration,
    multiplier: f64,
    jitter: f64,
    max_retries: Option<u32>,
}

impl Default for BackoffPolicy {
    /// 500 ms doubling to 30 s, 50% jitter, unlimited retries.
    fn default() -> Self {
        Self {
            initial: Duration::from_millis(500),
            max: Duration::from_secs(30),
            multiplier: 2.0,
            jitter: 0.5,
            max_retries: None,
        }
    }
}

impl BackoffPolicy {
    /// Backoff from `initial` up to `max` (raised to `initial` if lower),
    /// doubling, 50% jitter, unlimited retries.
    pub fn new(initial: Duration, max: Duration) -> Self {
        Self {
            initial,
            max: max.max(initial),
            ..Self::default()
        }
    }

    /// Growth factor per retry (clamped to at least 1).
    pub fn with_multiplier(mut self, multiplier: f64) -> Self {
        self.multiplier = if multiplier.is_finite() {
            multiplier.max(1.0)
        } else {
            1.0
        };
        self
    }

    /// Random reduction, as a fraction of the delay (clamped to `0..=1`;
    /// `0` disables jitter).
    pub fn with_jitter(mut self, jitter: f64) -> Self {
        self.jitter = if jitter.is_finite() {
            jitter.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self
    }

    /// Give up after `retries` consecutive failed attempts without progress
    /// and return the last error. Default: retry until cancelled.
    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = Some(retries);
        self
    }

    /// Initial delay.
    pub fn initial(&self) -> Duration {
        self.initial
    }

    /// Delay cap.
    pub fn max(&self) -> Duration {
        self.max
    }

    /// Maximum consecutive retries, if bounded.
    pub fn max_retries(&self) -> Option<u32> {
        self.max_retries
    }

    /// Un-jittered delay before retry `attempt` (1-based).
    pub fn base_delay(&self, attempt: u32) -> Duration {
        let exp = attempt.saturating_sub(1).min(1024) as i32;
        let secs = self.initial.as_secs_f64() * self.multiplier.powi(exp);
        if !secs.is_finite() || secs >= self.max.as_secs_f64() {
            self.max
        } else {
            Duration::from_secs_f64(secs).min(self.max)
        }
    }

    /// Jittered delay before retry `attempt` (1-based).
    pub fn delay(&self, attempt: u32) -> Duration {
        let base = self.base_delay(attempt);
        base.mul_f64(1.0 - self.jitter * unit_random())
    }
}

/// Uniform random number in `[0, 1)` without a `rand` dependency:
/// `RandomState` is randomly keyed, and the counter varies each call.
fn unit_random() -> f64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    (hasher.finish() >> 11) as f64 / (1u64 << 53) as f64
}

/// What a runner is doing, for health checks.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum RunnerState {
    /// Not started yet.
    #[default]
    Idle,
    /// Checking server capabilities and loading the checkpoint.
    Starting,
    /// Replaying history up to the live boundary.
    CatchingUp,
    /// Consuming live events.
    Live,
    /// Waiting to reconnect after a transient failure.
    Backoff {
        /// Consecutive failed attempts so far.
        attempt: u32,
        /// Delay before the next attempt.
        delay: Duration,
    },
    /// Stopped at an undecodable stored event (ADR-026), waiting for an
    /// operator. With a recheck interval the runner re-checks on its own.
    Halted {
        /// Position of the undecodable event.
        global_nonce: u64,
    },
    /// Stopped cleanly (cancelled).
    Stopped,
    /// Stopped with a non-retryable error (see `last_error`).
    Failed,
}

/// Observable health of a [`ProjectionRunner`]; see
/// [`ProjectionRunner::health`].
#[derive(Debug, Clone, Default)]
pub struct RunnerHealth {
    /// Current state.
    pub state: RunnerState,
    /// Last committed position (0 = nothing processed).
    pub position: u64,
    /// Head of the tenant log observed when the current attempt started.
    pub live_boundary: Option<u64>,
    /// Set while halted at an undecodable stored event, including during
    /// re-check attempts. Clears once the runner is past that position.
    pub halted_at: Option<u64>,
    /// Message of the most recent error (kept after recovery, for
    /// diagnostics). The typed error is what `run` / `run_supervised`
    /// returns.
    pub last_error: Option<String>,
    /// Failed attempts since the runner last committed an event or went
    /// live. Non-zero means it is retrying, halted, or stopped by an error.
    pub consecutive_failures: u32,
    /// Attempts started after the first (reconnects and re-checks).
    pub restarts: u64,
}

impl RunnerHealth {
    /// Events between the checkpoint and the live boundary while catching
    /// up; `None` otherwise (live lag is not measured).
    pub fn lag(&self) -> Option<u64> {
        match self.state {
            RunnerState::CatchingUp => self
                .live_boundary
                .map(|boundary| boundary.saturating_sub(self.position)),
            _ => None,
        }
    }

    /// True while catching up or live, not halted, and with no failure since
    /// the last progress. Expose this (and `halted_at`) to readiness checks
    /// and alerts.
    pub fn is_healthy(&self) -> bool {
        matches!(self.state, RunnerState::CatchingUp | RunnerState::Live)
            && self.halted_at.is_none()
            && self.consecutive_failures == 0
    }
}

/// How the supervisor treats a failed attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Failure {
    /// Reconnect from the checkpoint after a backoff.
    Transient,
    /// Undecodable stored event: halt, never retry with backoff.
    DataLoss(u64),
    /// Stop with the error.
    Fatal,
}

/// Classify an attempt's error for supervision.
///
/// Transient: event store `UNAVAILABLE`, `RESOURCE_EXHAUSTED` (a lagging
/// subscription), `DEADLINE_EXCEEDED`, transport failures (`UNKNOWN`,
/// `INTERNAL`, `CANCELLED` as surfaced by HTTP/2), and connection-level
/// failures of the Postgres projection store, including one a handler hit
/// writing through the store transaction (wrapped in `ProjectionFailed`).
/// Everything else is fatal: handler and upcast errors (`ProjectionFailed`),
/// fencing, incompatibility, configuration, out-of-order delivery, processor
/// panics, decode errors, and unrecognized store errors.
pub(super) fn classify(err: &Error) -> Failure {
    if let Some(position) = err.data_loss_position() {
        return Failure::DataLoss(position);
    }
    let transient = match err {
        // The event was rolled back with its transaction; a lost database
        // connection is an outage, not a bug in the handler.
        Error::ProjectionFailed { source, .. } => is_transient_store_error(source),
        _ => err.is_transient() || is_transient_store_error(err),
    };
    if transient {
        Failure::Transient
    } else {
        Failure::Fatal
    }
}

#[cfg(feature = "postgres")]
fn is_transient_store_error(err: &Error) -> bool {
    let Error::Repository(inner) = err else {
        return false;
    };
    let Some(sql) = inner
        .chain()
        .find_map(|cause| cause.downcast_ref::<sqlx::Error>())
    else {
        return false;
    };
    match sql {
        sqlx::Error::Io(_) | sqlx::Error::PoolTimedOut | sqlx::Error::Protocol(_) => true,
        sqlx::Error::Database(db) => db.code().is_some_and(|code| {
            // 08: connection exception; 57P01-03: server shutting down or
            // cannot connect now; 40001/40P01: serialization/deadlock.
            code.starts_with("08")
                || matches!(&*code, "57P01" | "57P02" | "57P03" | "40001" | "40P01")
        }),
        _ => false,
    }
}

#[cfg(not(feature = "postgres"))]
fn is_transient_store_error(_err: &Error) -> bool {
    false
}

impl<P, S> ProjectionRunner<P, S>
where
    P: CheckpointedProjection<S>,
    S: ProjectionStore,
{
    /// [`run`](Self::run) under supervision, until `cancel` fires or a
    /// non-retryable error occurs.
    ///
    /// * **Transient failures** (see [`BackoffPolicy`]; event store
    ///   `UNAVAILABLE`, `RESOURCE_EXHAUSTED`, transport errors, Postgres
    ///   connection loss) reconnect from the persisted checkpoint after a
    ///   jittered exponential backoff. Committed events are skipped on
    ///   redelivery, so nothing is lost or applied twice. The backoff resets
    ///   after progress.
    /// * **Undecodable stored event** (`DATA_LOSS`, ADR-026): never retried
    ///   with backoff and never skipped. The runner halts at the position
    ///   (one `ERROR` log per position; [`RunnerHealth::halted_at`] set; the
    ///   live processor is held). By default it returns
    ///   [`Error::DataLoss`]. With
    ///   [`with_undecodable_recheck`](Self::with_undecodable_recheck) it stays
    ///   halted and re-checks at that fixed interval, resuming on its own once
    ///   an operator repaired the row or moved the checkpoint past it.
    /// * **Everything else** stops with the error: handler or upcast failures
    ///   ([`Error::ProjectionFailed`]), fencing ([`Error::CheckpointFenced`]),
    ///   an incompatible server ([`Error::Incompatible`]),
    ///   [`Error::OutOfOrderDelivery`], [`Error::LiveProcessorPanicked`].
    ///
    /// Cancellation is prompt, including during a backoff or re-check wait.
    pub async fn run_supervised(
        &mut self,
        cancel: CancellationToken,
        policy: BackoffPolicy,
    ) -> Result<RunExit> {
        // Consecutive failed attempts without progress; drives the backoff.
        let mut failures: u32 = 0;
        let mut first = true;
        loop {
            if cancel.is_cancelled() {
                self.set_state(RunnerState::Stopped);
                return Ok(RunExit::Cancelled {
                    position: self.position,
                });
            }
            if !first {
                self.health.send_modify(|h| h.restarts += 1);
            }
            first = false;
            let err = match self.run(cancel.clone()).await {
                Ok(exit) => return Ok(exit),
                Err(err) => err,
            };
            // Progress: the attempt committed an event, or its live phase
            // stayed up for at least the backoff cap.
            let stable_live = self
                .live_since
                .is_some_and(|since| since.elapsed() >= policy.max());
            if self.position > self.loaded_position || stable_live {
                failures = 0;
            }
            failures = failures.saturating_add(1);
            let wait = match classify(&err) {
                Failure::Fatal => {
                    tracing::error!(projection = %self.key, error = %err, "projection runner stopped");
                    return Err(err);
                }
                // `run` entered the halt and logged it (once per position).
                Failure::DataLoss(global_nonce) => match self.undecodable_recheck {
                    None => return Err(err),
                    Some(interval) => {
                        tracing::debug!(
                            projection = %self.key,
                            global_nonce,
                            recheck_in = ?interval,
                            "halted at undecodable stored event; re-checking"
                        );
                        interval
                    }
                },
                Failure::Transient => {
                    if policy.max_retries().is_some_and(|max| failures > max) {
                        tracing::error!(
                            projection = %self.key,
                            error = %err,
                            failures,
                            "projection runner giving up after max retries"
                        );
                        return Err(err);
                    }
                    let delay = policy.delay(failures);
                    tracing::warn!(
                        projection = %self.key,
                        error = %err,
                        attempt = failures,
                        retry_in = ?delay,
                        position = self.position,
                        "projection runner failed; reconnecting from checkpoint"
                    );
                    match self.halted_at {
                        // A re-check attempt failed transiently: still halted.
                        Some(global_nonce) => self.set_state(RunnerState::Halted { global_nonce }),
                        None => self.set_state(RunnerState::Backoff {
                            attempt: failures,
                            delay,
                        }),
                    }
                    delay
                }
            };
            tokio::select! {
                biased;
                _ = cancel.cancelled() => {
                    self.set_state(RunnerState::Stopped);
                    return Ok(RunExit::Cancelled { position: self.position });
                }
                _ = tokio::time::sleep(wait) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_exponentially_and_caps() {
        let p =
            BackoffPolicy::new(Duration::from_millis(100), Duration::from_secs(2)).with_jitter(0.0);
        let delays: Vec<_> = (1..=7).map(|n| p.delay(n)).collect();
        assert_eq!(
            delays,
            [100, 200, 400, 800, 1600, 2000, 2000].map(Duration::from_millis)
        );
        // No overflow or panic for absurd attempt counts.
        assert_eq!(p.delay(u32::MAX), Duration::from_secs(2));
        assert_eq!(p.base_delay(0), Duration::from_millis(100));
    }

    #[test]
    fn jittered_delays_stay_within_bounds_and_vary() {
        let p =
            BackoffPolicy::new(Duration::from_millis(100), Duration::from_secs(1)).with_jitter(0.5);
        let mut seen = std::collections::HashSet::new();
        for attempt in 1..=10 {
            let base = p.base_delay(attempt);
            for _ in 0..200 {
                let d = p.delay(attempt);
                assert!(d <= base && d <= p.max(), "{d:?} > {base:?}");
                assert!(
                    d >= base.mul_f64(0.5),
                    "{d:?} below jitter floor of {base:?}"
                );
                seen.insert(d);
            }
        }
        assert!(seen.len() > 100, "jitter must vary delays");
    }

    #[test]
    fn policy_inputs_are_sanitized() {
        let p = BackoffPolicy::new(Duration::from_secs(5), Duration::from_secs(1))
            .with_multiplier(f64::NAN)
            .with_jitter(7.0);
        assert_eq!(p.max(), Duration::from_secs(5), "max raised to initial");
        assert!(p.delay(3) <= Duration::from_secs(5));
        let p = BackoffPolicy::default()
            .with_multiplier(0.1)
            .with_jitter(-1.0);
        assert_eq!(
            p.delay(4),
            p.initial(),
            "multiplier clamped to 1, no jitter"
        );
    }

    #[test]
    fn classification() {
        use tonic::Status;
        let transient = [
            Status::unavailable("x"),
            Status::resource_exhausted("lagged"),
            Status::deadline_exceeded("x"),
            Status::unknown("h2 reset"),
            Status::internal("x"),
            Status::cancelled("x"),
        ];
        for status in transient {
            assert_eq!(classify(&Error::from(status)), Failure::Transient);
        }
        let fatal = [
            Status::invalid_argument("x"),
            Status::permission_denied("x"),
            Status::unauthenticated("x"),
            Status::not_found("x"),
            Status::data_loss("no position metadata"),
        ];
        for status in fatal {
            assert_eq!(classify(&Error::from(status)), Failure::Fatal);
        }
        let mut md = tonic::metadata::MetadataMap::new();
        md.insert(crate::error::UNDECODABLE_GLOBAL_NONCE_KEY, 9u64.into());
        let data_loss = Status::with_metadata(tonic::Code::DataLoss, "bad", md);
        assert_eq!(classify(&Error::from(data_loss)), Failure::DataLoss(9));
        for err in [
            Error::ProjectionFailed {
                projection: "p".into(),
                global_nonce: 1,
                source: Box::new(Error::from(Status::unavailable("handler"))),
            },
            Error::CheckpointFenced {
                projection: "p".into(),
                stored: 2,
                position: 2,
            },
            Error::LiveProcessorPanicked {
                projection: "p".into(),
                message: "boom".into(),
            },
            Error::OutOfOrderDelivery {
                projection: "p".into(),
                last_applied: 3,
                received: 2,
            },
            Error::Config("x".into()),
            Error::Repository(anyhow::anyhow!("unknown store error")),
        ] {
            assert_eq!(classify(&err), Failure::Fatal, "{err:?}");
        }
    }

    #[cfg(feature = "postgres")]
    #[test]
    fn postgres_store_connection_failures_are_transient() {
        let wrap = |e: sqlx::Error| Error::Repository(anyhow::Error::new(e));
        assert_eq!(
            classify(&wrap(sqlx::Error::PoolTimedOut)),
            Failure::Transient
        );
        let io = std::io::Error::new(std::io::ErrorKind::ConnectionReset, "reset");
        assert_eq!(classify(&wrap(sqlx::Error::Io(io))), Failure::Transient);
        assert_eq!(classify(&wrap(sqlx::Error::RowNotFound)), Failure::Fatal);
        assert_eq!(classify(&wrap(sqlx::Error::PoolClosed)), Failure::Fatal);
        // A handler that hit a lost connection through the store transaction.
        let in_handler = |source: Error| Error::ProjectionFailed {
            projection: "p".into(),
            global_nonce: 1,
            source: Box::new(source),
        };
        let reset = std::io::Error::new(std::io::ErrorKind::ConnectionReset, "reset");
        assert_eq!(
            classify(&in_handler(wrap(sqlx::Error::Io(reset)))),
            Failure::Transient
        );
        let with_context = Error::Repository(
            anyhow::Error::new(sqlx::Error::PoolTimedOut).context("upsert balance"),
        );
        assert_eq!(classify(&in_handler(with_context)), Failure::Transient);
        assert_eq!(
            classify(&in_handler(wrap(sqlx::Error::RowNotFound))),
            Failure::Fatal
        );
        assert_eq!(
            classify(&in_handler(Error::domain("business rule"))),
            Failure::Fatal
        );
    }

    #[test]
    fn health_lag_and_readiness() {
        let mut h = RunnerHealth {
            state: RunnerState::CatchingUp,
            position: 3,
            live_boundary: Some(10),
            ..RunnerHealth::default()
        };
        assert_eq!(h.lag(), Some(7));
        assert!(h.is_healthy());
        h.halted_at = Some(4);
        assert!(!h.is_healthy());
        h.halted_at = None;
        h.state = RunnerState::Live;
        assert_eq!(h.lag(), None);
        assert!(h.is_healthy());
        h.consecutive_failures = 1;
        assert!(!h.is_healthy());
    }
}
