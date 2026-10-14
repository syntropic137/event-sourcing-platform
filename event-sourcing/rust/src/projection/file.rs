//! Durable local [`CheckpointStore`]; see [`FileCheckpointStore`].

use std::collections::{BTreeMap, VecDeque};
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::{fence_error, CheckpointKey, CheckpointStore};
use crate::error::{Error, Result};

const FILE: &str = "checkpoints.json";
const TMP: &str = "checkpoints.json.tmp";
const LOCK: &str = "LOCK";
const FORMAT: u32 = 1;

#[derive(Serialize, Deserialize)]
struct Entry {
    tenant_id: String,
    projection_name: String,
    projection_version: u32,
    feed: String,
    position: u64,
}

#[derive(Serialize, Deserialize)]
struct Document {
    format: u32,
    checkpoints: Vec<Entry>,
}

type Positions = BTreeMap<CheckpointKey, u64>;

type Change = Box<dyn FnOnce(&mut Positions) -> Result<()> + Send>;

/// A queued `save` or `delete` and where to report its outcome.
struct QueuedWrite {
    change: Change,
    done: tokio::sync::oneshot::Sender<Result<()>>,
}

struct Inner {
    dir: PathBuf,
    /// Current positions (what is on disk). Locked only briefly, never
    /// across I/O, so `load` never waits for a flush.
    positions: Mutex<Positions>,
    /// Serializes writers across the whole read-modify-write-flush.
    writer: Mutex<()>,
    /// Writes in call order. A write is queued before its blocking task is
    /// spawned, and every task drains the queue in order, so a write whose
    /// caller was dropped (a timeout) is still applied in its place and can
    /// never land after a later `delete` and resurrect the checkpoint.
    queue: Mutex<VecDeque<QueuedWrite>>,
    /// Held for the life of the store; dropping it releases the OS lock.
    _lock: File,
    /// Test seam: fail the next directory flush after a rename.
    #[cfg(test)]
    fail_next_dir_sync: std::sync::atomic::AtomicBool,
}

impl Inner {
    fn inject_dir_sync_failure(&self) -> bool {
        #[cfg(test)]
        {
            self.fail_next_dir_sync
                .swap(false, std::sync::atomic::Ordering::SeqCst)
        }
        #[cfg(not(test))]
        false
    }
}

/// Whether a rename is flushed to disk before `save`/`delete` return.
const DURABLE_RENAME: bool = cfg!(unix);

/// Durable local [`CheckpointStore`]: checkpoints in one file on disk, for a
/// process without Postgres (a desktop app, a CLI, a background job). Wrap it
/// in [`ExternalCheckpoints`](super::ExternalCheckpoints) so an external read
/// model resumes from where it stopped instead of replaying from position 0.
///
/// # Layout and guarantees
///
/// * One directory per store: `checkpoints.json` (every key's position) and
///   `LOCK`. [`FileCheckpointStore::open`] takes an exclusive OS lock on
///   `LOCK` for the life of the store, so a second store (in this or another
///   process) on the same directory is refused instead of racing it. The OS
///   releases the lock when the process exits, including on a crash.
/// * Every `save` and `delete` rewrites `checkpoints.json` atomically: write
///   a temporary file, flush it to disk, rename it over the old one, then
///   flush the directory. A crash or power loss leaves the old file or the new
///   one, never a torn one. A file that does not parse (or names a key twice)
///   is an error at `open`, never a silent reset to zero. Directories `open`
///   creates are flushed into their parents too.
/// * `save` is fenced like [`InMemoryCheckpointStore`](super::InMemoryCheckpointStore):
///   a position at or below the stored one fails with
///   [`Error::CheckpointFenced`] and writes nothing.
/// * A write that fails before the rename changes nothing. If only the final
///   directory flush fails, the new file is already in place: the store keeps
///   the new positions (what is on disk) and returns the error.
/// * Each write costs one or two `fsync`s, so a checkpoint per event during a
///   long catch-up is slow on some disks; the position is what you pay for.
/// * Windows (any non-Unix target): the rename is not flushed to disk
///   separately (std has no directory flush there). A power loss can then
///   restore the previous file. For `save` that rolls a checkpoint back and
///   redelivers events; handlers behind `ExternalCheckpoints` are idempotent
///   by contract. A file is still never torn. A rolled-back `delete` would
///   resurrect a position after `rebuild` cleared the read model, skipping
///   history, so there `delete` of a stored key is refused: to rebuild,
///   remove the directory while no store holds it.
///
/// Requires Rust 1.89+ (`std::fs::File::try_lock`).
///
/// Cheap to clone: clones share the lock, the positions and the file.
#[derive(Clone)]
pub struct FileCheckpointStore {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for FileCheckpointStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileCheckpointStore")
            .field("dir", &self.inner.dir)
            .finish()
    }
}

fn io(what: &str, path: &Path, err: std::io::Error) -> Error {
    Error::Repository(anyhow::anyhow!(
        "checkpoint store: {what} {}: {err}",
        path.display()
    ))
}

fn invalid(path: &Path, what: impl std::fmt::Display) -> Error {
    Error::Repository(anyhow::anyhow!(
        "checkpoint store: {}: {what}",
        path.display()
    ))
}

impl FileCheckpointStore {
    /// Open (creating if needed) the store in `dir` and lock it. Fails if
    /// another store holds the lock, or if `checkpoints.json` is unreadable.
    ///
    /// Blocking: call it at startup or from `spawn_blocking`.
    pub fn open(dir: impl AsRef<Path>) -> Result<Self> {
        // Absolute before creating (so the parent of a relative `dir` is
        // flushed too), canonical after (so a later working-directory change
        // cannot point the store at another directory than the one locked).
        let requested = dir.as_ref();
        let absolute = std::path::absolute(requested).map_err(|e| io("resolve", requested, e))?;
        create_dir_durably(&absolute)?;
        let dir = fs::canonicalize(&absolute).map_err(|e| io("resolve", &absolute, e))?;
        let lock_path = dir.join(LOCK);
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .map_err(|e| io("open", &lock_path, e))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(invalid(&dir, "in use by another store"));
            }
            Err(TryLockError::Error(e)) => return Err(io("lock", &lock_path, e)),
        }
        let positions = load(&dir.join(FILE))?;
        Ok(Self {
            inner: Arc::new(Inner {
                dir,
                positions: Mutex::new(positions),
                writer: Mutex::new(()),
                queue: Mutex::new(VecDeque::new()),
                _lock: lock,
                #[cfg(test)]
                fail_next_dir_sync: std::sync::atomic::AtomicBool::new(false),
            }),
        })
    }

    /// The store's directory.
    pub fn dir(&self) -> &Path {
        &self.inner.dir
    }

    fn current(&self) -> Positions {
        self.inner.positions.lock().expect("poisoned").clone()
    }

    /// Apply `change` to a copy of the positions, persist it, and make it
    /// current once it is on disk. A failed write before the rename changes
    /// nothing. Writes apply in call order, even when a caller stops waiting.
    async fn update(
        &self,
        change: impl FnOnce(&mut Positions) -> Result<()> + Send + 'static,
    ) -> Result<()> {
        let (done, result) = tokio::sync::oneshot::channel();
        self.inner
            .queue
            .lock()
            .expect("poisoned")
            .push_back(QueuedWrite {
                change: Box::new(change),
                done,
            });
        let store = self.clone();
        let drained = tokio::task::spawn_blocking(move || store.drain());
        match result.await {
            Ok(result) => result,
            // The write was taken off the queue but never answered: the
            // draining task panicked.
            Err(_) => Err(Error::Repository(anyhow::anyhow!(
                "checkpoint store task: {}",
                match drained.await {
                    Err(e) => e.to_string(),
                    Ok(()) => "write was not applied".to_string(),
                }
            ))),
        }
    }

    /// Apply every queued write, oldest first.
    fn drain(&self) {
        let _writer = self.inner.writer.lock().expect("poisoned");
        loop {
            let Some(write) = self.inner.queue.lock().expect("poisoned").pop_front() else {
                return;
            };
            let _ = write.done.send(self.apply(write.change));
        }
    }

    fn apply(&self, change: Change) -> Result<()> {
        let mut next = self.current();
        change(&mut next)?;
        let replaced = persist(&self.inner.dir, &next, self.inner.inject_dir_sync_failure());
        if replaced.renamed {
            *self.inner.positions.lock().expect("poisoned") = next;
        }
        replaced.result
    }
}

/// Create `dir` and any missing ancestors, flushing each new entry into its
/// parent so the store itself survives a power loss.
fn create_dir_durably(dir: &Path) -> Result<()> {
    let missing: Vec<&Path> = dir.ancestors().take_while(|p| !p.exists()).collect();
    fs::create_dir_all(dir).map_err(|e| io("create", dir, e))?;
    for created in missing.iter().rev() {
        if let Some(parent) = created.parent().filter(|p| !p.as_os_str().is_empty()) {
            sync_dir(parent)?;
        }
    }
    Ok(())
}

fn load(path: &Path) -> Result<Positions> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Positions::new()),
        Err(e) => return Err(io("read", path, e)),
    };
    let doc: Document = serde_json::from_slice(&bytes)
        .map_err(|e| invalid(path, format!("not a checkpoint file: {e}")))?;
    if doc.format != FORMAT {
        return Err(invalid(
            path,
            format!("format {}, expected {FORMAT}", doc.format),
        ));
    }
    let mut positions = Positions::new();
    for e in doc.checkpoints {
        let key = CheckpointKey::new(e.tenant_id, e.projection_name, e.projection_version)
            .with_feed(e.feed);
        if positions.insert(key.clone(), e.position).is_some() {
            return Err(invalid(path, format!("checkpoint {key} appears twice")));
        }
    }
    Ok(positions)
}

struct Persisted {
    /// The new file replaced the old one.
    renamed: bool,
    result: Result<()>,
}

fn persist(dir: &Path, positions: &Positions, fail_dir_sync: bool) -> Persisted {
    let before = |result| Persisted {
        renamed: false,
        result,
    };
    let doc = Document {
        format: FORMAT,
        checkpoints: positions
            .iter()
            .map(|(k, &position)| Entry {
                tenant_id: k.tenant_id.clone(),
                projection_name: k.projection_name.clone(),
                projection_version: k.projection_version,
                feed: k.feed.clone(),
                position,
            })
            .collect(),
    };
    let bytes = match serde_json::to_vec_pretty(&doc) {
        Ok(bytes) => bytes,
        Err(e) => return before(Err(invalid(dir, format!("encode: {e}")))),
    };
    let tmp = dir.join(TMP);
    let path = dir.join(FILE);
    let written = File::create(&tmp)
        .map_err(|e| io("create", &tmp, e))
        .and_then(|mut file| {
            file.write_all(&bytes).map_err(|e| io("write", &tmp, e))?;
            file.sync_all().map_err(|e| io("sync", &tmp, e))
        });
    if let Err(e) = written {
        return before(Err(e));
    }
    if let Err(e) = fs::rename(&tmp, &path) {
        return before(Err(io("rename", &path, e)));
    }
    Persisted {
        renamed: true,
        result: if fail_dir_sync {
            Err(invalid(dir, "injected directory flush failure"))
        } else {
            sync_dir(dir)
        },
    }
}

/// Flush a directory entry so a rename or create inside it survives a power
/// loss. Unix only; see the module docs for Windows.
fn sync_dir(dir: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        File::open(dir)
            .and_then(|d| d.sync_all())
            .map_err(|e| io("sync", dir, e))?;
    }
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

/// Remove `key`. Without a durable rename a power loss could bring the
/// position back after the caller cleared its read model, so a stored key
/// is refused there (see the module docs).
fn remove(positions: &mut Positions, key: &CheckpointKey, durable_rename: bool) -> Result<()> {
    if !durable_rename && positions.contains_key(key) {
        return Err(Error::Repository(anyhow::anyhow!(
            "checkpoint store: deleting {key} is not crash-safe on this platform; \
             remove the store's directory while no store holds it"
        )));
    }
    positions.remove(key);
    Ok(())
}

#[async_trait]
impl CheckpointStore for FileCheckpointStore {
    async fn load(&self, key: &CheckpointKey) -> Result<Option<u64>> {
        Ok(self
            .inner
            .positions
            .lock()
            .expect("poisoned")
            .get(key)
            .copied())
    }

    async fn save(&self, key: &CheckpointKey, position: u64) -> Result<()> {
        let key = key.clone();
        self.update(move |positions| {
            if let Some(&stored) = positions.get(&key) {
                if stored >= position {
                    return Err(fence_error(&key, stored, position));
                }
            }
            positions.insert(key, position);
            Ok(())
        })
        .await
    }

    async fn delete(&self, key: &CheckpointKey) -> Result<()> {
        let key = key.clone();
        self.update(move |positions| remove(positions, &key, DURABLE_RENAME))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A temporary directory removed on drop, even when an assertion fails.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!("esp-ckpt-{}", uuid::Uuid::new_v4())))
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
            }
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn key(name: &str) -> CheckpointKey {
        CheckpointKey::new("t", name, 1)
    }

    #[tokio::test]
    async fn positions_survive_reopen_and_nested_directories_are_created() {
        let tmp = TempDir::new();
        let dir = tmp.0.join("a").join("b");
        {
            let store = FileCheckpointStore::open(&dir).unwrap();
            store.save(&key("a"), 3).await.unwrap();
            store.save(&key("a"), 7).await.unwrap();
            store
                .save(&key("b").with_feed("order-\\%_/\"x\""), 2)
                .await
                .unwrap();
        }
        let store = FileCheckpointStore::open(&dir).unwrap();
        assert_eq!(store.load(&key("a")).await.unwrap(), Some(7));
        assert_eq!(
            store
                .load(&key("b").with_feed("order-\\%_/\"x\""))
                .await
                .unwrap(),
            Some(2)
        );
        assert_eq!(
            store.load(&key("b")).await.unwrap(),
            None,
            "feed is part of the key"
        );
    }

    #[tokio::test]
    async fn saves_are_fenced_and_a_fenced_save_writes_nothing() {
        let tmp = TempDir::new();
        let store = FileCheckpointStore::open(&tmp.0).unwrap();
        store.save(&key("a"), 5).await.unwrap();
        let err = store.save(&key("a"), 5).await.unwrap_err();
        assert!(
            matches!(
                err,
                Error::CheckpointFenced {
                    stored: 5,
                    position: 5,
                    ..
                }
            ),
            "{err}"
        );
        assert!(store.save(&key("a"), 4).await.is_err());
        drop(store);
        let store = FileCheckpointStore::open(&tmp.0).unwrap();
        assert_eq!(store.load(&key("a")).await.unwrap(), Some(5));
    }

    #[tokio::test]
    async fn delete_persists_and_other_keys_are_untouched() {
        let tmp = TempDir::new();
        let store = FileCheckpointStore::open(&tmp.0).unwrap();
        store.save(&key("a"), 5).await.unwrap();
        store.save(&key("b"), 9).await.unwrap();
        if !DURABLE_RENAME {
            // Refused, and nothing changes (see `remove`).
            assert!(store.delete(&key("a")).await.is_err());
            assert_eq!(store.load(&key("a")).await.unwrap(), Some(5));
            return;
        }
        store.delete(&key("a")).await.unwrap();
        store.delete(&key("missing")).await.unwrap();
        drop(store);
        let store = FileCheckpointStore::open(&tmp.0).unwrap();
        assert_eq!(store.load(&key("a")).await.unwrap(), None);
        assert_eq!(store.load(&key("b")).await.unwrap(), Some(9));
        store.save(&key("a"), 1).await.unwrap();
    }

    #[test]
    fn the_store_keeps_a_canonical_absolute_directory() {
        let tmp = TempDir::new();
        let store = FileCheckpointStore::open(tmp.0.join("x").join("..").join("y")).unwrap();
        assert!(store.dir().is_absolute());
        assert_eq!(store.dir(), fs::canonicalize(tmp.0.join("y")).unwrap());
    }

    #[test]
    fn a_second_store_on_the_same_directory_is_refused_until_the_first_drops() {
        let tmp = TempDir::new();
        let first = FileCheckpointStore::open(&tmp.0).unwrap();
        let err = FileCheckpointStore::open(&tmp.0).unwrap_err();
        assert!(err.to_string().contains("in use"), "{err}");
        drop(first);
        FileCheckpointStore::open(&tmp.0).unwrap();
    }

    #[test]
    fn a_corrupt_or_ambiguous_file_is_an_error_not_a_reset() {
        let tmp = TempDir::new();
        fs::create_dir_all(&tmp.0).unwrap();
        let file = tmp.0.join(FILE);
        fs::write(&file, b"{ not json").unwrap();
        let err = FileCheckpointStore::open(&tmp.0).unwrap_err();
        assert!(err.to_string().contains("not a checkpoint file"), "{err}");
        fs::write(&file, br#"{"format":99,"checkpoints":[]}"#).unwrap();
        let err = FileCheckpointStore::open(&tmp.0).unwrap_err();
        assert!(err.to_string().contains("format 99"), "{err}");
        let entry = r#"{"tenant_id":"t","projection_name":"a","projection_version":1,"feed":"","position":"#;
        fs::write(
            &file,
            format!(r#"{{"format":1,"checkpoints":[{entry}9}},{entry}2}}]}}"#),
        )
        .unwrap();
        let err = FileCheckpointStore::open(&tmp.0).unwrap_err();
        assert!(err.to_string().contains("appears twice"), "{err}");
    }

    #[tokio::test]
    async fn a_leftover_temp_file_from_a_crash_is_ignored_and_replaced() {
        let tmp = TempDir::new();
        {
            let store = FileCheckpointStore::open(&tmp.0).unwrap();
            store.save(&key("a"), 4).await.unwrap();
        }
        fs::write(tmp.0.join(TMP), b"torn").unwrap();
        let store = FileCheckpointStore::open(&tmp.0).unwrap();
        assert_eq!(store.load(&key("a")).await.unwrap(), Some(4));
        store.save(&key("a"), 6).await.unwrap();
        drop(store);
        let store = FileCheckpointStore::open(&tmp.0).unwrap();
        assert_eq!(store.load(&key("a")).await.unwrap(), Some(6));
    }

    /// A write that fails before the rename leaves both disk and memory as
    /// they were, and the store recovers once the fault clears.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_failed_write_changes_nothing() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = TempDir::new();
        let store = FileCheckpointStore::open(&tmp.0).unwrap();
        store.save(&key("a"), 3).await.unwrap();
        fs::set_permissions(&tmp.0, fs::Permissions::from_mode(0o555)).unwrap();
        if File::create(tmp.0.join("probe")).is_ok() {
            // Running as root: permissions do not fault writes here.
            return;
        }

        let err = store.save(&key("a"), 4).await.unwrap_err();
        assert!(err.to_string().contains("checkpoint store"), "{err}");
        assert_eq!(store.load(&key("a")).await.unwrap(), Some(3));

        fs::set_permissions(&tmp.0, fs::Permissions::from_mode(0o755)).unwrap();
        store.save(&key("a"), 4).await.unwrap();
        drop(store);
        let store = FileCheckpointStore::open(&tmp.0).unwrap();
        assert_eq!(store.load(&key("a")).await.unwrap(), Some(4));
    }

    /// Only the directory flush after the rename fails: the new file is in
    /// place, so memory follows it, the error is returned, later saves are
    /// fenced against it, and a reopen reads it.
    #[tokio::test]
    async fn a_failed_flush_after_the_rename_keeps_what_is_on_disk() {
        let tmp = TempDir::new();
        let store = FileCheckpointStore::open(&tmp.0).unwrap();
        store.save(&key("a"), 5).await.unwrap();
        store
            .inner
            .fail_next_dir_sync
            .store(true, std::sync::atomic::Ordering::SeqCst);

        let err = store.save(&key("a"), 7).await.unwrap_err();
        assert!(err.to_string().contains("injected"), "{err}");
        assert_eq!(store.load(&key("a")).await.unwrap(), Some(7));
        assert!(matches!(
            store.save(&key("a"), 6).await.unwrap_err(),
            Error::CheckpointFenced { stored: 7, .. }
        ));

        drop(store);
        let store = FileCheckpointStore::open(&tmp.0).unwrap();
        assert_eq!(store.load(&key("a")).await.unwrap(), Some(7));
        store.save(&key("a"), 8).await.unwrap();
    }

    #[test]
    fn without_a_durable_rename_deleting_a_stored_key_is_refused() {
        let mut positions = Positions::new();
        positions.insert(key("a"), 5);
        let err = remove(&mut positions, &key("a"), false).unwrap_err();
        assert!(err.to_string().contains("not crash-safe"), "{err}");
        assert_eq!(positions.get(&key("a")), Some(&5));
        remove(&mut positions, &key("missing"), false).unwrap();
        remove(&mut positions, &key("a"), true).unwrap();
        assert!(positions.is_empty());
    }

    /// A save whose caller stopped waiting (a timeout) is still applied in
    /// call order, so a later delete wins and the checkpoint stays gone.
    #[cfg(unix)]
    #[tokio::test]
    // Holding the writer across awaits is the point: it parks the save's
    // blocking task while the caller drops it and queues a delete.
    #[allow(clippy::await_holding_lock)]
    async fn a_dropped_save_never_lands_after_a_later_delete() {
        let tmp = TempDir::new();
        let store = FileCheckpointStore::open(&tmp.0).unwrap();
        store.save(&key("a"), 5).await.unwrap();

        // Hold the writer so the save is queued but cannot run yet.
        let writer = store.inner.writer.lock().unwrap();
        let a = key("a");
        let mut save = Box::pin(store.save(&a, 9));
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), &mut save)
                .await
                .is_err(),
            "the save waits for the writer"
        );
        drop(save);
        let delete = tokio::spawn({
            let store = store.clone();
            async move { store.delete(&key("a")).await }
        });
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        drop(writer);
        delete.await.unwrap().unwrap();

        assert_eq!(store.load(&key("a")).await.unwrap(), None);
        drop(store);
        let store = FileCheckpointStore::open(&tmp.0).unwrap();
        assert_eq!(store.load(&key("a")).await.unwrap(), None);
    }

    /// A queued write whose own task never ran (its caller and task are
    /// gone) is applied, in order, by the next write's task.
    #[cfg(unix)]
    #[tokio::test]
    async fn an_orphaned_queued_write_is_applied_before_later_ones() {
        let tmp = TempDir::new();
        let store = FileCheckpointStore::open(&tmp.0).unwrap();
        store.save(&key("a"), 5).await.unwrap();
        let (done, _) = tokio::sync::oneshot::channel();
        store.inner.queue.lock().unwrap().push_back(QueuedWrite {
            change: Box::new(|positions: &mut Positions| {
                positions.insert(key("a"), 9);
                Ok(())
            }),
            done,
        });

        // The delete comes after the orphaned save: it wins.
        store.delete(&key("a")).await.unwrap();
        assert_eq!(store.load(&key("a")).await.unwrap(), None);
        drop(store);
        let store = FileCheckpointStore::open(&tmp.0).unwrap();
        assert_eq!(store.load(&key("a")).await.unwrap(), None);
    }
}
