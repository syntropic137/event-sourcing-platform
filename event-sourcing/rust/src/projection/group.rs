//! A bounded context's projection runners hosted together (#436, #448).
//!
//! A context usually owns several views and one or more to-do-list
//! automations (Dilger's Processor-TODO-List, *Understanding Eventsourcing*,
//! pp. 495-509). [`RunnerGroup`] hosts their [`ProjectionRunner`]s as one
//! unit without merging them: each runner keeps its own checkpoint key, store,
//! processor and settings, like a processing group of its own (pp. 270, 418).
//! The group owns no to-do list and issues no command; the jobs belong to the
//! context whose command they issue (pp. 506-509).
//!
//! Two modes:
//!
//! * **Batch** ([`RunnerGroup::run_until_idle`]): sweep every runner's
//!   [`ProjectionRunner::run_until_idle`] until one whole sweep processes
//!   nothing and no event is appended to any runner's log while it runs. That fixed point covers
//!   automations that feed each other (A's command creates work for B, B's for
//!   A), which one pass over the runners cannot prove.
//! * **Live** ([`RunnerGroup::spawn_supervised`]): one
//!   [`ProjectionRunner::run_supervised`] task per runner, with per-runner
//!   health and one shutdown.
//!
//! # Durable checkpoints for external read models
//!
//! Views whose read model outlives the process (a file, a search index) can
//! share one [`FileCheckpointStore`](super::FileCheckpointStore) behind
//! [`ExternalCheckpoints`](super::ExternalCheckpoints): open it once and hand
//! each runner a clone of the `Arc`. A read model held in memory must **not**
//! use it: its state is gone after a restart, so its checkpoint must go too;
//! give it an [`InMemoryProjectionStore`](super::InMemoryProjectionStore) and it
//! rebuilds from the log.
//!
//! ```no_run
//! # use std::sync::Arc;
//! # use event_sourcing_rust::projection::{ExternalCheckpoints, FileCheckpointStore};
//! # fn open() -> event_sourcing_rust::error::Result<()> {
//! let checkpoints = Arc::new(ExternalCheckpoints::new(FileCheckpointStore::open(
//!     "/var/lib/my-service/checkpoints",
//! )?));
//! // ProjectionRunner::new(port.clone(), checkpoints.clone(), search_index, "tenant")
//! # Ok(()) }
//! ```

use std::future::Future;
use std::pin::Pin;

use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::{
    BackoffPolicy, CheckpointedProjection, IdleExit, ProjectionRunner, ProjectionStore, RunExit,
    RunnerHealth,
};
use crate::error::Result;

/// Default bound on sweeps in one [`RunnerGroup::run_until_idle`] call.
const DEFAULT_MAX_SWEEPS: u32 = 100;

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A [`ProjectionRunner`] with its projection and store types erased.
trait GroupMember: Send + Sync {
    fn name(&self) -> String;
    fn run_until_idle(&mut self, cancel: CancellationToken) -> BoxFuture<'_, Result<IdleExit>>;
    fn head(&self) -> BoxFuture<'_, Result<u64>>;
    fn spawn_supervised(
        self: Box<Self>,
        cancel: CancellationToken,
        policy: BackoffPolicy,
    ) -> RunningMember;
}

impl<P, S> GroupMember for ProjectionRunner<P, S>
where
    P: CheckpointedProjection<S> + Sync + 'static,
    S: ProjectionStore + 'static,
{
    fn name(&self) -> String {
        self.key().to_string()
    }

    fn run_until_idle(&mut self, cancel: CancellationToken) -> BoxFuture<'_, Result<IdleExit>> {
        Box::pin(ProjectionRunner::run_until_idle(self, cancel))
    }

    fn head(&self) -> BoxFuture<'_, Result<u64>> {
        Box::pin(ProjectionRunner::head(self))
    }

    fn spawn_supervised(
        self: Box<Self>,
        cancel: CancellationToken,
        policy: BackoffPolicy,
    ) -> RunningMember {
        let mut runner = *self;
        let name = runner.key().to_string();
        let health = runner.health();
        let task = tokio::spawn(async move { runner.run_supervised(cancel, policy).await });
        RunningMember { name, health, task }
    }
}

/// One runner at rest after [`RunnerGroup::run_until_idle`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberIdle {
    /// The runner's checkpoint key, rendered.
    pub name: String,
    /// Last committed position.
    pub position: u64,
    /// Items its processor reported over the whole call (0 without one).
    pub processed: usize,
    /// Processor passes over the whole call (0 without a processor).
    pub passes: u32,
}

/// Why [`RunnerGroup::run_until_idle`] returned without error.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum GroupIdleExit {
    /// One whole sweep processed nothing and no runner's log head changed
    /// during it. One entry per runner, in the order they were added.
    Idle {
        /// Per-runner totals.
        members: Vec<MemberIdle>,
        /// Sweeps run, including the final quiet one.
        sweeps: u32,
    },
    /// The cancellation token fired first.
    Cancelled,
    /// A runner's processor still reported work at its pass bound
    /// ([`ProjectionRunner::with_idle_max_passes`]).
    PassLimit {
        /// The runner's checkpoint key, rendered.
        name: String,
        /// Items it reported over that runner call.
        processed: usize,
    },
    /// The group was still not at rest after the sweep bound
    /// ([`RunnerGroup::with_max_sweeps`]): runners keep producing work for
    /// each other, or another writer keeps appending.
    SweepLimit {
        /// Per-runner totals so far.
        members: Vec<MemberIdle>,
    },
}

/// Projection runners hosted together. See the [module docs](self).
#[derive(Default)]
pub struct RunnerGroup {
    members: Vec<Box<dyn GroupMember>>,
    max_sweeps: Option<u32>,
}

impl std::fmt::Debug for RunnerGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunnerGroup")
            .field(
                "members",
                &self.members.iter().map(|m| m.name()).collect::<Vec<_>>(),
            )
            .field("max_sweeps", &self.max_sweeps())
            .finish()
    }
}

impl RunnerGroup {
    /// An empty group.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a runner. It keeps its own checkpoint, store, processor and
    /// settings. In batch mode runners are swept in the order they were
    /// added; any order reaches the same rest, an order with automations
    /// before the views that read their events needs fewer sweeps.
    pub fn with<P, S>(mut self, runner: ProjectionRunner<P, S>) -> Self
    where
        P: CheckpointedProjection<S> + Sync + 'static,
        S: ProjectionStore + 'static,
    {
        self.members.push(Box::new(runner));
        self
    }

    /// Bound the sweeps of one [`run_until_idle`](Self::run_until_idle) call
    /// (at least 1, default 100). Reaching it returns
    /// [`GroupIdleExit::SweepLimit`].
    pub fn with_max_sweeps(mut self, sweeps: u32) -> Self {
        self.max_sweeps = Some(sweeps.max(1));
        self
    }

    fn max_sweeps(&self) -> u32 {
        self.max_sweeps.unwrap_or(DEFAULT_MAX_SWEEPS)
    }

    /// Number of runners.
    pub fn len(&self) -> usize {
        self.members.len()
    }

    /// True without runners.
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// Batch mode: bring the whole group to rest, then return.
    ///
    /// Each sweep runs every runner's
    /// [`run_until_idle`](ProjectionRunner::run_until_idle) in order. The
    /// group is at rest after a sweep in which no processor took anything and
    /// no runner's log head moved (checked before and after the sweep): nothing
    /// was appended during it, by these processors or anyone else, and every
    /// runner caught up to that head inside it. A processor that writes events
    /// but reports 0 is therefore still caught. An error from any runner is returned at
    /// once; every checkpoint stays where it committed, so the next call
    /// resumes. Runners can be swept again by later calls.
    pub async fn run_until_idle(&mut self, cancel: CancellationToken) -> Result<GroupIdleExit> {
        let mut members: Vec<MemberIdle> = self
            .members
            .iter()
            .map(|m| MemberIdle {
                name: m.name(),
                position: 0,
                processed: 0,
                passes: 0,
            })
            .collect();
        for sweep in 1..=self.max_sweeps() {
            if cancel.is_cancelled() {
                return Ok(GroupIdleExit::Cancelled);
            }
            // The log head each runner reads, before the sweep. If no runner's
            // head moved by the end of the sweep, nothing was appended during
            // it (by these processors or anyone else), and every runner caught
            // up to that same head inside the sweep.
            let before = self.heads(&cancel).await?;
            let Some(before) = before else {
                return Ok(GroupIdleExit::Cancelled);
            };
            let mut took = 0usize;
            for (member, total) in self.members.iter_mut().zip(members.iter_mut()) {
                match member.run_until_idle(cancel.clone()).await? {
                    IdleExit::Idle {
                        position,
                        processed,
                        passes,
                    } => {
                        total.position = position;
                        total.processed += processed;
                        total.passes += passes;
                        took += processed;
                    }
                    IdleExit::Cancelled { .. } => return Ok(GroupIdleExit::Cancelled),
                    IdleExit::PassLimit { processed, .. } => {
                        return Ok(GroupIdleExit::PassLimit {
                            name: total.name.clone(),
                            processed,
                        })
                    }
                }
            }
            let Some(after) = self.heads(&cancel).await? else {
                return Ok(GroupIdleExit::Cancelled);
            };
            if took == 0 && after == before {
                return Ok(GroupIdleExit::Idle {
                    members,
                    sweeps: sweep,
                });
            }
        }
        Ok(GroupIdleExit::SweepLimit { members })
    }

    /// Every runner's tenant log head, or `None` if cancelled first.
    async fn heads(&self, cancel: &CancellationToken) -> Result<Option<Vec<u64>>> {
        let mut heads = Vec::with_capacity(self.members.len());
        for member in &self.members {
            let head = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Ok(None),
                head = member.head() => head?,
            };
            heads.push(head);
        }
        Ok(Some(heads))
    }

    /// Live mode: spawn one [`run_supervised`](ProjectionRunner::run_supervised)
    /// task per runner on the current Tokio runtime, each with a child of
    /// `cancel`. Must be called from within a Tokio runtime.
    pub fn spawn_supervised(
        self,
        cancel: CancellationToken,
        policy: BackoffPolicy,
    ) -> RunningGroup {
        // The group's own token: shutdown cancels these runners, never the caller's.
        let cancel = cancel.child_token();
        let members = self
            .members
            .into_iter()
            .map(|m| m.spawn_supervised(cancel.child_token(), policy.clone()))
            .collect();
        RunningGroup { members, cancel }
    }
}

struct RunningMember {
    name: String,
    health: watch::Receiver<RunnerHealth>,
    task: JoinHandle<Result<RunExit>>,
}

/// A [`RunnerGroup`] running live. Dropping it does not stop the runners;
/// call [`shutdown`](Self::shutdown) (or cancel the token it was spawned with).
pub struct RunningGroup {
    members: Vec<RunningMember>,
    cancel: CancellationToken,
}

/// How one runner of a [`RunningGroup`] ended.
#[derive(Debug)]
pub struct MemberExit {
    /// The runner's checkpoint key, rendered.
    pub name: String,
    /// Its health when it ended.
    pub health: RunnerHealth,
    /// What `run_supervised` returned. A task that panicked or was aborted is
    /// `Err(Error::Repository)` naming the join failure.
    pub result: Result<RunExit>,
}

impl RunningGroup {
    /// Current health of every runner, in the order they were added.
    pub fn health(&self) -> Vec<(String, RunnerHealth)> {
        self.members
            .iter()
            .map(|m| (m.name.clone(), m.health.borrow().clone()))
            .collect()
    }

    /// Cancel every runner (each finishes or rolls back its in-flight event,
    /// and gives an in-flight processor pass its shutdown grace), wait for all
    /// of them, and return how each ended, in the order they were added.
    pub async fn shutdown(self) -> Vec<MemberExit> {
        self.cancel.cancel();
        let mut exits = Vec::with_capacity(self.members.len());
        for RunningMember { name, health, task } in self.members {
            let result = match task.await {
                Ok(result) => result,
                Err(join) => Err(crate::error::Error::Repository(anyhow::anyhow!(
                    "runner task for {name} ended abnormally: {join}"
                ))),
            };
            let health = health.borrow().clone();
            exits.push(MemberExit {
                name,
                health,
                result,
            });
        }
        exits
    }
}
