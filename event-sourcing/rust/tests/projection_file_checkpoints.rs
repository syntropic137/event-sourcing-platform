//! Live-server tests for `FileCheckpointStore`: an external read model behind
//! `ExternalCheckpoints` resumes across process restarts instead of replaying
//! the feed from position 0 (#438).

mod common;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use common::{connect, spawn_server, unique_tenant};
use event_sourcing_rust::client::{proto, EventStoreClient, EventStorePort};
use event_sourcing_rust::error::Result;
use event_sourcing_rust::projection::{
    CheckpointKey, CheckpointStore, CheckpointedProjection, DispatchContext, ExternalCheckpoints,
    FileCheckpointStore, ProjectionRunner, RecordedEvent,
};

type Store = ExternalCheckpoints<FileCheckpointStore>;

/// An "external" read model: the global nonces it has seen.
struct Seen {
    nonces: Arc<Mutex<Vec<u64>>>,
}

#[async_trait]
impl CheckpointedProjection<Store> for Seen {
    fn name(&self) -> &str {
        "seen"
    }
    fn version(&self) -> u32 {
        1
    }
    async fn handle(
        &mut self,
        _tx: &mut (),
        event: &RecordedEvent,
        _ctx: &DispatchContext,
    ) -> Result<()> {
        self.nonces.lock().unwrap().push(event.global_nonce);
        Ok(())
    }
    async fn reset(&mut self, _tx: &mut (), _key: &CheckpointKey) -> Result<()> {
        self.nonces.lock().unwrap().clear();
        Ok(())
    }
}

async fn append(client: &EventStoreClient, tenant: &str, id: &str, nonce: u64) {
    client
        .append(proto::AppendRequest {
            tenant_id: tenant.into(),
            aggregate_id: id.into(),
            aggregate_type: "Account".into(),
            expected_aggregate_nonce: nonce - 1,
            idempotency_key: String::new(),
            events: vec![proto::EventData {
                meta: Some(proto::EventMetadata {
                    event_id: uuid::Uuid::new_v4().to_string(),
                    aggregate_id: id.into(),
                    aggregate_type: "Account".into(),
                    aggregate_nonce: nonce,
                    event_type: "Deposited".into(),
                    event_version: 1,
                    content_type: "application/json".into(),
                    tenant_id: tenant.into(),
                    ..Default::default()
                }),
                payload: b"{}".to_vec(),
            }],
        })
        .await
        .expect("append");
}

/// A temporary directory removed on drop, even when an assertion fails.
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_dir() -> TempDir {
    TempDir(std::env::temp_dir().join(format!("esp-file-ckpt-{}", uuid::Uuid::new_v4())))
}

fn runner(
    port: &Arc<dyn EventStorePort>,
    dir: &PathBuf,
    tenant: &str,
    nonces: &Arc<Mutex<Vec<u64>>>,
) -> ProjectionRunner<Seen, Store> {
    let store = FileCheckpointStore::open(dir).expect("open store");
    ProjectionRunner::new(
        port.clone(),
        Arc::new(ExternalCheckpoints::new(store)),
        Seen {
            nonces: nonces.clone(),
        },
        tenant,
    )
}

#[tokio::test]
async fn a_restarted_runner_resumes_from_the_file_checkpoint() {
    let server = spawn_server().await;
    let client = connect(&server.addr).await;
    let port: Arc<dyn EventStorePort> = Arc::new(client.clone());
    let tenant = unique_tenant();
    let tmp = temp_dir();
    let dir = tmp.0.clone();
    for nonce in 1..=3 {
        append(&client, &tenant, "a", nonce).await;
    }

    let first_seen = Arc::new(Mutex::new(Vec::new()));
    {
        let mut first = runner(&port, &dir, &tenant, &first_seen);
        assert_eq!(first.catch_up().await.unwrap(), 3);
    } // runner and store dropped: the "process" exits, releasing the lock
    assert_eq!(*first_seen.lock().unwrap(), vec![1, 2, 3]);

    append(&client, &tenant, "a", 4).await;
    append(&client, &tenant, "b", 1).await;

    let second_seen = Arc::new(Mutex::new(Vec::new()));
    let mut second = runner(&port, &dir, &tenant, &second_seen);
    assert_eq!(second.catch_up().await.unwrap(), 5);
    assert_eq!(
        *second_seen.lock().unwrap(),
        vec![4, 5],
        "only events after the persisted checkpoint are redelivered"
    );

    // A rebuild removes the persisted checkpoint too. Without a durable
    // rename (non-Unix) the store refuses the delete and keeps it.
    let rebuilt = second.rebuild().await;
    drop(second);
    let store = FileCheckpointStore::open(&dir).unwrap();
    let stored = store
        .load(&CheckpointKey::new(&tenant, "seen", 1))
        .await
        .unwrap();
    if cfg!(unix) {
        rebuilt.unwrap();
        assert_eq!(stored, None);
    } else {
        // Refused before the read model is reset: both are kept.
        assert!(rebuilt.is_err());
        assert_eq!(stored, Some(5));
        assert_eq!(*second_seen.lock().unwrap(), vec![4, 5]);
    }
}
