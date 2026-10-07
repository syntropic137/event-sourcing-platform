//! A projection service: a supervised `ProjectionRunner` with a live-only
//! side-effect processor and a health endpoint's worth of state.
//!
//! * `run_supervised` reconnects from the checkpoint after outages
//!   (jittered backoff), halts on an undecodable stored event (ADR-026), and
//!   stops on bugs (handler errors, processor panics) with typed errors.
//! * The runner refuses an event store that lacks the ordering and
//!   error-surfacing guarantees it relies on (capability guard).
//! * `health()` is what a readiness probe or metrics exporter reads.
//!
//! Uses `EVENT_STORE_ADDR` (e.g. `127.0.0.1:50051`) when set; otherwise starts
//! an in-memory event store in-process. Runs until Ctrl-C with `--serve`,
//! otherwise exits after a short demo.
//!
//! ```bash
//! cargo run --example supervised_projection
//! EVENT_STORE_ADDR=127.0.0.1:50051 cargo run --example supervised_projection -- --serve
//! ```

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use event_sourcing_rust::client::proto;
use event_sourcing_rust::error::Error;
use event_sourcing_rust::prelude::*;
use event_sourcing_rust::projection::InMemoryTx;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Deposited {
    amount: i64,
}
impl EventSchema for Deposited {
    const EVENT_TYPE: &'static str = "Deposited";
}

/// Read model: total deposits, plus a to-do per large deposit.
#[derive(Debug, Clone, Default)]
struct Totals {
    total: i64,
    to_notify: Vec<String>,
}

type Store = InMemoryProjectionStore<Totals>;

struct TotalsProjection;

#[async_trait]
impl CheckpointedProjection<Store> for TotalsProjection {
    fn name(&self) -> &str {
        "deposit-totals"
    }
    fn version(&self) -> u32 {
        1
    }
    fn handles(&self, event_type: &str) -> bool {
        event_type == Deposited::EVENT_TYPE
    }
    async fn handle(
        &mut self,
        tx: &mut InMemoryTx<Totals>,
        event: &RecordedEvent,
        _ctx: &DispatchContext,
    ) -> Result<()> {
        // Pure: update the read model and record what needs doing.
        let Deposited { amount } = event.decode()?;
        tx.state.total += amount;
        if amount >= 100 {
            tx.state.to_notify.push(event.event_id.clone());
        }
        Ok(())
    }
    async fn reset(&mut self, tx: &mut InMemoryTx<Totals>, _key: &CheckpointKey) -> Result<()> {
        tx.state = Totals::default();
        Ok(())
    }
}

/// Side effects, only while live. Idempotent: `sent` stands in for a
/// durable dedup table.
struct Notifier {
    store: Arc<Store>,
    key: CheckpointKey,
    sent: Mutex<HashSet<String>>,
}

#[async_trait]
impl LiveProcessor for Notifier {
    async fn process_pending(&self) -> Result<usize> {
        let pending = self.store.state(&self.key).to_notify;
        let mut sent = self.sent.lock().unwrap();
        let mut n = 0;
        for id in pending {
            if sent.insert(id.clone()) {
                println!("  notify: large deposit {id}");
                n += 1;
            }
        }
        Ok(n)
    }
}

async fn deposit(client: &EventStoreClient, tenant: &str, nonce: u64, amount: i64) -> Result<()> {
    let payload = serde_json::to_vec(&Deposited { amount })?;
    client
        .append(proto::AppendRequest {
            tenant_id: tenant.into(),
            aggregate_id: "account-1".into(),
            aggregate_type: "Account".into(),
            expected_aggregate_nonce: nonce - 1,
            idempotency_key: String::new(),
            events: vec![proto::EventData {
                meta: Some(proto::EventMetadata {
                    event_id: Uuid::new_v4().to_string(),
                    aggregate_id: "account-1".into(),
                    aggregate_type: "Account".into(),
                    aggregate_nonce: nonce,
                    event_type: Deposited::EVENT_TYPE.into(),
                    event_version: 1,
                    content_type: "application/json".into(),
                    tenant_id: tenant.into(),
                    ..Default::default()
                }),
                payload,
            }],
        })
        .await?;
    Ok(())
}

async fn start_in_process_server() -> String {
    let port = portpicker::pick_unused_port().expect("free port");
    let addr = format!("127.0.0.1:{port}");
    let socket = addr.parse().expect("socket addr");
    let service = eventstore_bin::Service {
        store: eventstore_backend_memory::InMemoryStore::new(),
    };
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(eventstore_bin::EventStoreServer::new(service))
            .serve(socket)
            .await
            .expect("serve");
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    addr
}

#[tokio::main]
async fn main() -> Result<()> {
    let serve = std::env::args().any(|a| a == "--serve");
    let addr = match std::env::var("EVENT_STORE_ADDR") {
        Ok(addr) => addr,
        Err(_) => start_in_process_server().await,
    };
    let client = EventStoreClient::connect(&addr).await?;
    let tenant = format!("example-{}", Uuid::new_v4());
    deposit(&client, &tenant, 1, 50).await?;
    deposit(&client, &tenant, 2, 250).await?;

    let store = Arc::new(Store::new());
    let runner = ProjectionRunner::new(
        Arc::new(client.clone()),
        store.clone(),
        TotalsProjection,
        &tenant,
    );
    let key = runner.key().clone();
    let notifier = Arc::new(Notifier {
        store: store.clone(),
        key: key.clone(),
        sent: Mutex::new(HashSet::new()),
    });
    let mut runner = runner
        .with_live_processor(notifier)
        // Production default: return Error::DataLoss and let the service
        // alert and exit. With a re-check interval the runner stays halted
        // and resumes on its own after an operator fix.
        .with_undecodable_recheck(Duration::from_secs(30));

    // A readiness probe / metrics exporter would read this.
    let mut health = runner.health();
    tokio::spawn(async move {
        while health.changed().await.is_ok() {
            let h = health.borrow_and_update().clone();
            println!(
                "health: {:?} position={} lag={:?} halted_at={:?} healthy={} last_error={:?}",
                h.state,
                h.position,
                h.lag(),
                h.halted_at,
                h.is_healthy(),
                h.last_error
            );
        }
    });

    let cancel = CancellationToken::new();
    let shutdown = cancel.clone();
    tokio::spawn(async move {
        if serve {
            let _ = tokio::signal::ctrl_c().await;
        } else {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        shutdown.cancel();
    });
    let demo = tokio::spawn({
        let client = client.clone();
        let tenant = tenant.clone();
        async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            deposit(&client, &tenant, 3, 500).await
        }
    });

    let policy = BackoffPolicy::new(Duration::from_millis(500), Duration::from_secs(30));
    match runner.run_supervised(cancel, policy).await {
        Ok(exit) => println!("stopped: {exit:?}"),
        Err(Error::DataLoss { global_nonce, .. }) => {
            eprintln!("halted at undecodable event {global_nonce}; see ADR-026");
            std::process::exit(2);
        }
        Err(err) => {
            eprintln!("projection service failed: {err}");
            std::process::exit(1);
        }
    }
    demo.await.expect("demo task")?;
    println!("total deposits: {}", store.state(&key).total);
    Ok(())
}
