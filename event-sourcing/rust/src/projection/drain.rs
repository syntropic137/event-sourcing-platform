//! Background task running a [`LiveProcessor`] one pass at a time, with
//! panic supervision.

use std::any::Any;
use std::future::Future;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::sync::{watch, Notify};
use tokio_util::sync::CancellationToken;

use super::LiveProcessor;

/// What a runner does when a [`LiveProcessor`] pass panics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProcessorPanicPolicy {
    /// Stop the runner with
    /// [`Error::LiveProcessorPanicked`](crate::error::Error::LiveProcessorPanicked)
    /// (default). A supervised runner does not retry it: a panic is a bug,
    /// not an outage.
    #[default]
    Stop,
    /// Log the panic at `ERROR` and retry the pass with the processor retry
    /// backoff, like a pass that returned an error. Pending items stay
    /// pending; `process_pending` must tolerate being re-run after a panic.
    Restart,
}

const MAX_RETRY: Duration = Duration::from_secs(30);

pub(super) struct Drain {
    wake: Arc<Notify>,
    stop: CancellationToken,
    handle: Option<tokio::task::JoinHandle<()>>,
    panicked: watch::Receiver<Option<String>>,
}

impl Drain {
    /// Spawn the drain task. `paused` holds every pass while set (the runner
    /// is halted at an undecodable event); wake the drain after clearing it.
    pub(super) fn spawn(
        processor: Arc<dyn LiveProcessor>,
        stop: CancellationToken,
        retry: Duration,
        paused: Arc<AtomicBool>,
        policy: ProcessorPanicPolicy,
        projection: String,
    ) -> Self {
        let wake = Arc::new(Notify::new());
        let (panic_tx, panicked) = watch::channel(None);
        let task_wake = wake.clone();
        let task_stop = stop.clone();
        let handle = tokio::spawn(async move {
            // Reports the task ending for any reason other than a requested
            // stop (including a panic outside a pass), so the runner never
            // keeps running with a dead processor.
            let guard = ExitGuard {
                report: panic_tx,
                stop: task_stop.clone(),
            };
            // Some(delay) while the last pass failed: retry after `delay`
            // even if no new live event arrives.
            let mut retry_after: Option<Duration> = None;
            loop {
                match retry_after {
                    None => tokio::select! {
                        biased;
                        _ = task_stop.cancelled() => return,
                        _ = task_wake.notified() => {}
                    },
                    Some(delay) => tokio::select! {
                        biased;
                        _ = task_stop.cancelled() => return,
                        _ = task_wake.notified() => {}
                        _ = tokio::time::sleep(delay) => {}
                    },
                }
                // Checked right before the call: no side effects while the
                // runner is halted. The runner wakes the drain when the halt
                // clears, so nothing pending is stranded.
                if paused.load(Ordering::SeqCst) {
                    retry_after = None;
                    continue;
                }
                // Creating the future can panic too (a hand-written impl).
                let pass = catch_unwind(AssertUnwindSafe(|| processor.process_pending()));
                let outcome = match pass {
                    Ok(pass) => CatchUnwind(pass).await,
                    Err(payload) => Err(payload),
                };
                let failed = match outcome {
                    Ok(Ok(_)) => false,
                    Ok(Err(err)) => {
                        // Pending items stay pending and are retried.
                        tracing::warn!(projection = %projection, error = %err, "live processor pass failed");
                        true
                    }
                    Err(payload) => {
                        let message = panic_message(payload.as_ref());
                        tracing::error!(
                            projection = %projection,
                            panic = %message,
                            restart = policy == ProcessorPanicPolicy::Restart,
                            "live processor pass panicked"
                        );
                        if policy == ProcessorPanicPolicy::Stop {
                            guard.report.send_replace(Some(message));
                            return;
                        }
                        true
                    }
                };
                retry_after = failed.then(|| match retry_after {
                    None => retry,
                    Some(d) => d.saturating_mul(2).min(MAX_RETRY),
                });
            }
        });
        Self {
            wake,
            stop,
            handle: Some(handle),
            panicked,
        }
    }

    pub(super) fn wake(&self) {
        // Stores one permit: wake-ups during a pass coalesce into one more.
        self.wake.notify_one();
    }

    /// Resolves with a message once the processor died: a pass panicked
    /// under [`ProcessorPanicPolicy::Stop`], or the task ended without being
    /// asked to. Pending forever otherwise.
    pub(super) async fn panicked(&mut self) -> String {
        loop {
            if let Some(message) = self.panicked.borrow_and_update().clone() {
                return message;
            }
            if self.panicked.changed().await.is_err() {
                // The guard reports before the sender drops; a closed channel
                // without a report means a requested stop: never resolves.
                if let Some(message) = self.panicked.borrow().clone() {
                    return message;
                }
                std::future::pending::<()>().await;
            }
        }
    }

    /// Let the current pass finish, then stop (graceful shutdown). Returns
    /// the failure message if the processor died, so a failure racing with
    /// shutdown is never lost.
    pub(super) async fn stop(mut self) -> Option<String> {
        self.stop.cancel();
        self.join().await
    }

    /// Stop now, cancelling an in-flight pass (the runner failed or halted:
    /// no side effect may start or keep running, and a pass blocked on an
    /// external service must not delay the halt). Safe because
    /// `process_pending` is idempotent; the next live pass redoes it.
    pub(super) async fn abort(mut self) -> Option<String> {
        self.stop.cancel();
        if let Some(handle) = self.handle.as_ref() {
            handle.abort();
        }
        self.join().await
    }

    async fn join(&mut self) -> Option<String> {
        // Await through a reference: if this future is dropped mid-wait
        // (run() aborted during shutdown), `Drain` still owns the handle and
        // `Drop` aborts the task instead of detaching it.
        let joined = match self.handle.as_mut() {
            Some(handle) => handle.await,
            None => Ok(()),
        };
        self.handle = None;
        if let Some(message) = self.panicked.borrow().clone() {
            return Some(message);
        }
        match joined {
            Err(err) if err.is_panic() => Some(panic_message(err.into_panic().as_ref())),
            _ => None,
        }
    }
}

/// Lives on the drain task; on drop (return, panic unwind, abort) reports an
/// unexpected end unless a stop was requested.
struct ExitGuard {
    report: watch::Sender<Option<String>>,
    stop: CancellationToken,
}

impl Drop for ExitGuard {
    fn drop(&mut self) {
        if !self.stop.is_cancelled() && self.report.borrow().is_none() {
            let message = if std::thread::panicking() {
                "live processor task panicked outside a pass".to_string()
            } else {
                "live processor task ended unexpectedly".to_string()
            };
            self.report.send_replace(Some(message));
        }
    }
}

impl Drop for Drain {
    /// `run()` was dropped or aborted without a graceful stop: the processor
    /// must not outlive its runner. Cancel and abort the task (an interrupted
    /// pass is safe because `process_pending` is idempotent).
    fn drop(&mut self) {
        self.stop.cancel();
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

/// Polls a boxed future, turning a panic into `Err(payload)`.
struct CatchUnwind<'a, T>(Pin<Box<dyn Future<Output = T> + Send + 'a>>);

impl<T> Future for CatchUnwind<'_, T> {
    type Output = std::thread::Result<T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let fut = self.0.as_mut();
        match catch_unwind(AssertUnwindSafe(|| fut.poll(cx))) {
            Ok(Poll::Pending) => Poll::Pending,
            Ok(Poll::Ready(value)) => Poll::Ready(Ok(value)),
            Err(payload) => Poll::Ready(Err(payload)),
        }
    }
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_string()
    }
}
