//! Aggregate repository against a live event store.
//!
//! Uses `EVENT_STORE_ADDR` (e.g. `127.0.0.1:50051`) when set; otherwise starts
//! an in-memory event store server in-process.
//!
//! ```bash
//! cargo run --example repository
//! # or, against a running server:
//! EVENT_STORE_ADDR=127.0.0.1:50051 cargo run --example repository
//! ```

use std::sync::Arc;

use event_sourcing_rust::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
enum CounterEvent {
    Created { id: String },
    Incremented { by: u32 },
}

impl DomainEvent for CounterEvent {
    fn event_type(&self) -> &'static str {
        match self {
            CounterEvent::Created { .. } => "CounterCreated",
            CounterEvent::Incremented { .. } => "CounterIncremented",
        }
    }
}

#[derive(Debug, Default)]
struct Counter {
    id: Option<String>,
    value: u64,
    version: u64,
}

impl Aggregate for Counter {
    type Event = CounterEvent;
    type Error = Error;

    fn aggregate_id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    fn aggregate_type(&self) -> &'static str {
        "Counter"
    }

    fn version(&self) -> u64 {
        self.version
    }

    fn apply_event(&mut self, event: &CounterEvent) -> Result<()> {
        match event {
            CounterEvent::Created { id } => self.id = Some(id.clone()),
            CounterEvent::Incremented { by } => self.value += u64::from(*by),
        }
        self.version += 1;
        Ok(())
    }
}

#[derive(Debug)]
enum CounterCommand {
    Create { id: String },
    Increment { by: u32 },
}

impl Command for CounterCommand {}

#[async_trait]
impl AggregateRoot for Counter {
    type Command = CounterCommand;

    async fn handle_command(&self, command: CounterCommand) -> Result<Vec<CounterEvent>> {
        match command {
            CounterCommand::Create { id } if self.id.is_none() => {
                Ok(vec![CounterEvent::Created { id }])
            }
            CounterCommand::Create { .. } => Err(Error::invalid_command("already created")),
            CounterCommand::Increment { .. } if self.id.is_none() => {
                Err(Error::invalid_command("counter does not exist"))
            }
            CounterCommand::Increment { by } => Ok(vec![CounterEvent::Incremented { by }]),
        }
    }
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
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    addr
}

#[tokio::main]
async fn main() -> Result<()> {
    let addr = match std::env::var("EVENT_STORE_ADDR") {
        Ok(addr) => addr,
        Err(_) => start_in_process_server().await,
    };
    let client = EventStoreClient::connect(&addr).await?;
    let repo = EventStoreRepository::<Counter>::new(Arc::new(client), "example-tenant");
    let id = format!("counter-{}", Uuid::new_v4());

    // Create and save.
    let mut counter = AggregateInstance::new(id.clone(), Counter::default());
    counter
        .execute(CounterCommand::Create { id: id.clone() })
        .await?;
    counter.execute(CounterCommand::Increment { by: 2 }).await?;
    repo.save(&mut counter).await?;
    println!("saved {id} at version {}", counter.committed_version());

    // Two writers load the same version; the second save is rejected.
    let mut a = repo.load(&id).await?.expect("exists");
    let mut b = repo.load(&id).await?.expect("exists");
    a.execute(CounterCommand::Increment { by: 1 }).await?;
    b.execute(CounterCommand::Increment { by: 10 }).await?;
    repo.save(&mut a).await?;
    match repo.save(&mut b).await {
        Err(Error::ConcurrencyConflict { expected, actual }) => {
            println!("stale writer rejected: expected {expected}, store at {actual}");
        }
        other => panic!("expected a concurrency conflict, got {other:?}"),
    }

    let current = repo.load(&id).await?.expect("exists");
    println!(
        "reloaded {id}: value {} at version {}",
        current.aggregate.value,
        current.committed_version()
    );
    Ok(())
}
