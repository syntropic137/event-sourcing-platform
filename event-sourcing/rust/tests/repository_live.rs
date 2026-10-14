//! Live-server tests for `EventStoreRepository` against the real gRPC event store.

mod common;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use common::{connect, spawn_server, unique_tenant, FaultyPort};
use event_sourcing_rust::client::{proto, EventStorePort};
use event_sourcing_rust::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Opened {
    id: String,
    owner: String,
}
impl EventSchema for Opened {
    const EVENT_TYPE: &'static str = "AccountOpened";
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Deposited {
    amount: i64,
}
impl EventSchema for Deposited {
    const EVENT_TYPE: &'static str = "MoneyDeposited";
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Poison {}
impl EventSchema for Poison {
    const EVENT_TYPE: &'static str = "Poison";
}

event_sourcing_rust::event_enum! {
    #[derive(Debug, Clone, PartialEq)]
    enum AccountEvent {
        Opened(Opened),
        Deposited(Deposited),
        Poison(Poison),
    }
}

#[derive(Debug, Clone, Default)]
struct Account {
    id: Option<String>,
    owner: String,
    balance: i64,
    version: u64,
}

impl Aggregate for Account {
    type Event = AccountEvent;
    type Error = Error;
    const AGGREGATE_TYPE: &'static str = "Account";

    fn aggregate_id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    fn version(&self) -> u64 {
        self.version
    }

    fn apply_event(&mut self, event: &AccountEvent) -> Result<()> {
        match event {
            AccountEvent::Opened(Opened { id, owner }) => {
                self.id = Some(id.clone());
                self.owner = owner.clone();
            }
            AccountEvent::Deposited(Deposited { amount }) => self.balance += amount,
            AccountEvent::Poison(_) => return Err(Error::invalid_state("poisoned")),
        }
        self.version += 1;
        Ok(())
    }
}

#[derive(Debug)]
enum AccountCommand {
    Open { id: String, owner: String },
    Deposit { amount: i64 },
    DepositThenPoison { amount: i64 },
}

impl Command for AccountCommand {}

#[async_trait]
impl AggregateRoot for Account {
    type Command = AccountCommand;

    async fn handle_command(&self, command: AccountCommand) -> Result<Vec<AccountEvent>> {
        match command {
            AccountCommand::Open { id, owner } => {
                if self.id.is_some() {
                    return Err(Error::invalid_command("already open"));
                }
                Ok(vec![Opened { id, owner }.into()])
            }
            AccountCommand::Deposit { amount } => {
                if self.id.is_none() {
                    return Err(Error::invalid_command("not open"));
                }
                Ok(vec![Deposited { amount }.into()])
            }
            AccountCommand::DepositThenPoison { amount } => {
                Ok(vec![Deposited { amount }.into(), Poison {}.into()])
            }
        }
    }
}

fn open(id: &str) -> AccountCommand {
    AccountCommand::Open {
        id: id.into(),
        owner: "alice".into(),
    }
}

/// Same events, different aggregate type: its streams are not Account's.
#[derive(Debug, Clone, Default)]
struct Savings(Account);

impl Aggregate for Savings {
    type Event = AccountEvent;
    type Error = Error;
    const AGGREGATE_TYPE: &'static str = "Savings";

    fn aggregate_id(&self) -> Option<&str> {
        self.0.aggregate_id()
    }

    fn version(&self) -> u64 {
        self.0.version()
    }

    fn apply_event(&mut self, event: &AccountEvent) -> Result<()> {
        self.0.apply_event(event)
    }
}

async fn opened_with_deposit(id: &str) -> AggregateInstance<Account> {
    let mut acct = AggregateInstance::new(id.to_string(), Account::default());
    acct.execute(open(id)).await.unwrap();
    acct.execute(AccountCommand::Deposit { amount: 10 })
        .await
        .unwrap();
    acct
}

async fn stream_len(store: &dyn EventStorePort, tenant: &str, id: &str) -> usize {
    store
        .read_stream(proto::ReadStreamRequest {
            tenant_id: tenant.into(),
            aggregate_id: id.into(),
            from_aggregate_nonce: 1,
            max_count: 1000,
            forward: true,
        })
        .await
        .unwrap()
        .events
        .len()
}

#[tokio::test]
async fn create_save_reload() {
    let server = spawn_server().await;
    let store: Arc<dyn EventStorePort> = Arc::new(connect(&server.addr).await);
    let tenant = unique_tenant();
    let repo = EventStoreRepository::<Account>::new(store.clone(), &tenant);

    let mut acct = opened_with_deposit("acct-1").await;
    assert_eq!(acct.committed_version(), 0);
    repo.save(&mut acct).await.expect("first save");
    assert!(!acct.has_uncommitted_events());
    assert_eq!(acct.committed_version(), 2);
    assert!(repo.exists("acct-1").await.unwrap());

    let mut loaded = repo.load("acct-1").await.unwrap().expect("exists");
    assert_eq!(loaded.committed_version(), 2);
    assert_eq!(loaded.aggregate.balance, 10);
    assert_eq!(loaded.aggregate.owner, "alice");

    // A second save uses the reloaded revision as its expectation.
    loaded
        .execute(AccountCommand::Deposit { amount: 5 })
        .await
        .unwrap();
    repo.save(&mut loaded).await.expect("second save");
    assert_eq!(loaded.committed_version(), 3);

    let reloaded = repo.load("acct-1").await.unwrap().unwrap();
    assert_eq!(reloaded.committed_version(), 3);
    assert_eq!(reloaded.aggregate.balance, 15);

    // Saving with nothing pending is a no-op.
    let mut clean = reloaded;
    repo.save(&mut clean).await.unwrap();
    assert_eq!(stream_len(store.as_ref(), &tenant, "acct-1").await, 3);
}

#[tokio::test]
async fn paged_replay_rebuilds_long_streams() {
    let server = spawn_server().await;
    let store: Arc<dyn EventStorePort> = Arc::new(connect(&server.addr).await);
    let repo = EventStoreRepository::<Account>::new(store, unique_tenant()).with_page_size(2);

    let mut acct = AggregateInstance::new("acct-p".to_string(), Account::default());
    acct.execute(open("acct-p")).await.unwrap();
    for _ in 0..6 {
        acct.execute(AccountCommand::Deposit { amount: 1 })
            .await
            .unwrap();
    }
    repo.save(&mut acct).await.unwrap();

    let loaded = repo.load("acct-p").await.unwrap().unwrap();
    assert_eq!(loaded.committed_version(), 7);
    assert_eq!(loaded.aggregate.balance, 6);
}

#[tokio::test]
async fn missing_aggregate() {
    let server = spawn_server().await;
    let store: Arc<dyn EventStorePort> = Arc::new(connect(&server.addr).await);
    let repo = EventStoreRepository::<Account>::new(store, unique_tenant());

    assert!(repo.load("nope").await.unwrap().is_none());
    assert!(!repo.exists("nope").await.unwrap());
}

#[tokio::test]
async fn tenants_are_isolated() {
    let server = spawn_server().await;
    let store: Arc<dyn EventStorePort> = Arc::new(connect(&server.addr).await);
    let repo_a = EventStoreRepository::<Account>::new(store.clone(), unique_tenant());
    let repo_b = EventStoreRepository::<Account>::new(store, unique_tenant());

    let mut acct = opened_with_deposit("shared-id").await;
    repo_a.save(&mut acct).await.unwrap();
    assert!(repo_b.load("shared-id").await.unwrap().is_none());
}

#[tokio::test]
async fn stale_writer_gets_typed_conflict_and_keeps_pending() {
    let server = spawn_server().await;
    let store: Arc<dyn EventStorePort> = Arc::new(connect(&server.addr).await);
    let tenant = unique_tenant();
    let repo = EventStoreRepository::<Account>::new(store.clone(), &tenant);

    let mut acct = opened_with_deposit("acct-2").await;
    repo.save(&mut acct).await.unwrap();

    let mut writer_a = repo.load("acct-2").await.unwrap().unwrap();
    let mut writer_b = repo.load("acct-2").await.unwrap().unwrap();
    writer_a
        .execute(AccountCommand::Deposit { amount: 1 })
        .await
        .unwrap();
    writer_b
        .execute(AccountCommand::Deposit { amount: 100 })
        .await
        .unwrap();

    repo.save(&mut writer_a).await.expect("first writer wins");
    let err = repo.save(&mut writer_b).await.expect_err("stale writer");
    match err {
        Error::ConcurrencyConflict { expected, actual } => {
            assert_eq!(expected, 2);
            assert_eq!(actual, 3);
        }
        other => panic!("expected ConcurrencyConflict, got {other:?}"),
    }
    assert_eq!(writer_b.uncommitted_count(), 1, "pending kept on conflict");

    let current = repo.load("acct-2").await.unwrap().unwrap();
    assert_eq!(current.aggregate.balance, 11, "stale write not applied");
    assert_eq!(stream_len(store.as_ref(), &tenant, "acct-2").await, 3);

    // The recovery path: reload, re-run the command, save.
    let mut retry = current;
    retry
        .execute(AccountCommand::Deposit { amount: 100 })
        .await
        .unwrap();
    repo.save(&mut retry).await.unwrap();
    assert_eq!(
        repo.load("acct-2")
            .await
            .unwrap()
            .unwrap()
            .aggregate
            .balance,
        111
    );
}

#[tokio::test]
async fn creating_an_existing_stream_conflicts() {
    let server = spawn_server().await;
    let store: Arc<dyn EventStorePort> = Arc::new(connect(&server.addr).await);
    let repo = EventStoreRepository::<Account>::new(store, unique_tenant());

    let mut first = opened_with_deposit("dup").await;
    repo.save(&mut first).await.unwrap();
    let mut second = opened_with_deposit("dup").await;
    let err = repo.save(&mut second).await.expect_err("must conflict");
    assert!(
        matches!(
            err,
            Error::ConcurrencyConflict {
                expected: 0,
                actual: 2
            }
        ),
        "{err:?}"
    );
}

#[tokio::test]
async fn uncertain_save_retried_by_caller_commits_exactly_once() {
    let server = spawn_server().await;
    let client = connect(&server.addr).await;
    let faulty = Arc::new(FaultyPort::new(client.clone()));
    faulty.lose_acks.store(1, Ordering::SeqCst);
    let tenant = unique_tenant();
    let repo = EventStoreRepository::<Account>::new(faulty.clone(), &tenant)
        .with_retry_policy(RetryPolicy::none());

    let mut acct = opened_with_deposit("acct-3").await;
    let err = repo.save(&mut acct).await.expect_err("ack lost");
    assert!(err.is_transient(), "{err:?}");
    assert_eq!(
        acct.uncommitted_count(),
        2,
        "pending kept when outcome unknown"
    );
    // The batch did commit server-side.
    assert_eq!(stream_len(&client, &tenant, "acct-3").await, 2);

    repo.save(&mut acct)
        .await
        .expect("retry recognizes own batch");
    assert!(!acct.has_uncommitted_events());
    assert_eq!(acct.committed_version(), 2);
    assert_eq!(faulty.appends_sent.load(Ordering::SeqCst), 2);
    assert_eq!(
        stream_len(&client, &tenant, "acct-3").await,
        2,
        "no duplicates"
    );

    // The instance keeps working after recovery.
    acct.execute(AccountCommand::Deposit { amount: 1 })
        .await
        .unwrap();
    repo.save(&mut acct).await.unwrap();
    let loaded = repo.load("acct-3").await.unwrap().unwrap();
    assert_eq!(loaded.committed_version(), 3);
    assert_eq!(loaded.aggregate.balance, 11);
}

#[tokio::test]
async fn uncertain_save_then_more_events_reconciles_committed_prefix() {
    let server = spawn_server().await;
    let client = connect(&server.addr).await;
    let faulty = Arc::new(FaultyPort::new(client.clone()));
    faulty.lose_acks.store(1, Ordering::SeqCst);
    let tenant = unique_tenant();
    let repo = EventStoreRepository::<Account>::new(faulty.clone(), &tenant)
        .with_retry_policy(RetryPolicy::none());

    let mut acct = opened_with_deposit("acct-8").await;
    repo.save(&mut acct).await.expect_err("ack lost");
    // The caller keeps working before retrying the save.
    acct.execute(AccountCommand::Deposit { amount: 5 })
        .await
        .unwrap();
    assert_eq!(acct.uncommitted_count(), 3);

    repo.save(&mut acct)
        .await
        .expect("committed prefix recognized");
    assert!(!acct.has_uncommitted_events());
    assert_eq!(acct.committed_version(), 3);
    assert_eq!(stream_len(&client, &tenant, "acct-8").await, 3);
    let loaded = repo.load("acct-8").await.unwrap().unwrap();
    assert_eq!(loaded.aggregate.balance, 15);
}

/// #362/#363 regression. Reconciliation used to mask two store bugs: an
/// identical retry with 2+ headers looked like a different payload, and the
/// memory backend checked the expected revision before the idempotency key.
/// Now the store itself answers the retry with the original ack, even with
/// many headers and after another writer advanced the stream, so the
/// repository needs no reconciliation read.
#[tokio::test]
async fn lost_ack_retry_with_headers_is_acked_by_store_after_stream_advanced() {
    let server = spawn_server().await;
    let client = connect(&server.addr).await;
    let faulty = Arc::new(FaultyPort::new(client.clone()));
    faulty.lose_acks.store(1, Ordering::SeqCst);
    let tenant = unique_tenant();
    let repo = EventStoreRepository::<Account>::new(faulty.clone(), &tenant)
        .with_retry_policy(RetryPolicy::none());

    let mut acct = opened_with_deposit("acct-hdr").await;
    for envelope in acct.uncommitted_events.iter_mut() {
        for i in 0..8 {
            envelope
                .metadata
                .metadata
                .insert(format!("x-header-{i}"), format!("value-{i}"));
        }
    }
    repo.save(&mut acct).await.expect_err("ack lost");
    assert_eq!(stream_len(&client, &tenant, "acct-hdr").await, 2);

    // Another writer advances the stream before the retry.
    let other_repo = EventStoreRepository::<Account>::new(Arc::new(client.clone()), &tenant);
    let mut other = other_repo.load("acct-hdr").await.unwrap().unwrap();
    other
        .execute(AccountCommand::Deposit { amount: 5 })
        .await
        .unwrap();
    other_repo.save(&mut other).await.unwrap();

    let reads_before = faulty.reads_sent.load(Ordering::SeqCst);
    repo.save(&mut acct)
        .await
        .expect("store returns the original ack");
    assert_eq!(
        faulty.reads_sent.load(Ordering::SeqCst),
        reads_before,
        "answered by the store, not by reconciliation"
    );
    assert!(!acct.has_uncommitted_events());
    assert_eq!(acct.committed_version(), 2);
    assert_eq!(stream_len(&client, &tenant, "acct-hdr").await, 3);
}

#[tokio::test]
async fn uncertain_save_retried_automatically_commits_exactly_once() {
    let server = spawn_server().await;
    let client = connect(&server.addr).await;
    let faulty = Arc::new(FaultyPort::new(client.clone()));
    faulty.lose_acks.store(1, Ordering::SeqCst);
    let tenant = unique_tenant();
    let repo = EventStoreRepository::<Account>::new(faulty.clone(), &tenant);

    let mut acct = opened_with_deposit("acct-4").await;
    repo.save(&mut acct).await.expect("auto retry succeeds");
    assert!(!acct.has_uncommitted_events());
    assert_eq!(faulty.appends_sent.load(Ordering::SeqCst), 2);
    assert_eq!(stream_len(&client, &tenant, "acct-4").await, 2);
}

#[tokio::test]
async fn dropped_request_is_retried() {
    let server = spawn_server().await;
    let client = connect(&server.addr).await;
    let faulty = Arc::new(FaultyPort::new(client.clone()));
    faulty.drop_requests.store(2, Ordering::SeqCst);
    let tenant = unique_tenant();
    let repo = EventStoreRepository::<Account>::new(faulty.clone(), &tenant);

    let mut acct = opened_with_deposit("acct-5").await;
    repo.save(&mut acct).await.expect("third attempt succeeds");
    assert_eq!(faulty.appends_sent.load(Ordering::SeqCst), 1);
    assert_eq!(stream_len(&client, &tenant, "acct-5").await, 2);
}

#[tokio::test]
async fn retries_exhausted_keeps_pending() {
    let server = spawn_server().await;
    let client = connect(&server.addr).await;
    let faulty = Arc::new(FaultyPort::new(client.clone()));
    faulty.drop_requests.store(10, Ordering::SeqCst);
    let repo = EventStoreRepository::<Account>::new(faulty.clone(), unique_tenant());

    let mut acct = opened_with_deposit("acct-6").await;
    let err = repo.save(&mut acct).await.expect_err("exhausted");
    assert!(err.is_transient());
    assert_eq!(acct.uncommitted_count(), 2);
}

#[tokio::test]
async fn aggregate_type_mismatch_is_an_error() {
    let server = spawn_server().await;
    let store: Arc<dyn EventStorePort> = Arc::new(connect(&server.addr).await);
    let tenant = unique_tenant();
    let repo = EventStoreRepository::<Account>::new(store.clone(), &tenant);
    let other = EventStoreRepository::<Savings>::new(store, &tenant);

    let mut acct = opened_with_deposit("acct-7").await;
    repo.save(&mut acct).await.unwrap();
    assert!(other.load("acct-7").await.is_err());
    assert!(other.exists("acct-7").await.is_err());

    // Saving through a repository of another type must not append.
    let mut loaded = AggregateInstance::from_history("acct-7".into(), Savings::default(), 2);
    loaded
        .add_events(vec![Deposited { amount: 1 }.into()])
        .unwrap();
    assert!(other.save(&mut loaded).await.is_err());
    assert_eq!(loaded.uncommitted_count(), 1);
    assert_eq!(
        repo.load("acct-7")
            .await
            .unwrap()
            .unwrap()
            .committed_version(),
        2
    );

    // Instance metadata is not trusted: a Savings stream rehydrated by hand
    // (metadata says "Account") must still be rejected by the Account repo.
    let mut savings = AggregateInstance::new("sav-1".to_string(), Savings::default());
    savings
        .add_events(vec![
            Opened {
                id: "sav-1".into(),
                owner: "bob".into(),
            }
            .into(),
            Deposited { amount: 10 }.into(),
        ])
        .unwrap();
    other.save(&mut savings).await.unwrap();
    let mut forged = AggregateInstance::from_history("sav-1".into(), Account::default(), 2);
    forged.execute(open("sav-1")).await.unwrap();
    assert!(repo.save(&mut forged).await.is_err());
    assert!(
        other.load("sav-1").await.unwrap().is_some(),
        "stream intact"
    );
    assert_eq!(
        other
            .load("sav-1")
            .await
            .unwrap()
            .unwrap()
            .committed_version(),
        2
    );
}

#[tokio::test]
async fn execute_is_atomic_when_an_event_fails_to_apply() {
    let server = spawn_server().await;
    let store: Arc<dyn EventStorePort> = Arc::new(connect(&server.addr).await);
    let repo = EventStoreRepository::<Account>::new(store, unique_tenant());

    let mut acct = opened_with_deposit("acct-9").await;
    repo.save(&mut acct).await.unwrap();
    let err = acct
        .execute(AccountCommand::DepositThenPoison { amount: 50 })
        .await
        .expect_err("second event fails to apply");
    assert!(
        matches!(err, Error::InvalidAggregateState { .. }),
        "{err:?}"
    );
    assert_eq!(acct.aggregate.balance, 10, "state rolled back");
    assert_eq!(acct.aggregate.version, 2);
    assert_eq!(acct.uncommitted_count(), 0);
    assert_eq!(acct.metadata.version, 2);

    // The instance stays consistent with the store.
    acct.execute(AccountCommand::Deposit { amount: 1 })
        .await
        .unwrap();
    repo.save(&mut acct).await.unwrap();
    let loaded = repo.load("acct-9").await.unwrap().unwrap();
    assert_eq!(loaded.aggregate.balance, acct.aggregate.balance);
    assert_eq!(loaded.aggregate.balance, 11);
}

// ---------------------------------------------------------------------------
// Wire format (ADR-027) and upcasting
// ---------------------------------------------------------------------------

async fn read_raw(store: &dyn EventStorePort, tenant: &str, id: &str) -> Vec<proto::EventData> {
    store
        .read_stream(proto::ReadStreamRequest {
            tenant_id: tenant.into(),
            aggregate_id: id.into(),
            from_aggregate_nonce: 1,
            max_count: 1000,
            forward: true,
        })
        .await
        .unwrap()
        .events
}

/// Append raw events as another SDK would: (event_type, version, payload).
async fn append_raw(
    store: &dyn EventStorePort,
    tenant: &str,
    id: &str,
    events: &[(&str, u32, &str)],
) {
    store
        .append(proto::AppendRequest {
            tenant_id: tenant.into(),
            aggregate_id: id.into(),
            aggregate_type: "Account".into(),
            expected_aggregate_nonce: 0,
            idempotency_key: String::new(),
            events: events
                .iter()
                .enumerate()
                .map(|(i, (ty, version, payload))| proto::EventData {
                    meta: Some(proto::EventMetadata {
                        event_id: uuid::Uuid::new_v4().to_string(),
                        aggregate_id: id.into(),
                        aggregate_type: "Account".into(),
                        aggregate_nonce: i as u64 + 1,
                        event_type: (*ty).into(),
                        event_version: *version,
                        content_type: "application/json".into(),
                        tenant_id: tenant.into(),
                        ..Default::default()
                    }),
                    payload: payload.as_bytes().to_vec(),
                })
                .collect(),
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn saved_events_use_the_cross_language_envelope() {
    let server = spawn_server().await;
    let store: Arc<dyn EventStorePort> = Arc::new(connect(&server.addr).await);
    let tenant = unique_tenant();
    let repo = EventStoreRepository::<Account>::new(store.clone(), &tenant);
    let mut acct = opened_with_deposit("acct-w").await;
    repo.save(&mut acct).await.unwrap();

    let raw = read_raw(store.as_ref(), &tenant, "acct-w").await;
    let metas: Vec<_> = raw.iter().map(|e| e.meta.clone().unwrap()).collect();
    assert_eq!(metas[0].event_type, "AccountOpened");
    assert_eq!(metas[1].event_type, "MoneyDeposited");
    for m in &metas {
        assert_eq!(m.event_version, 1);
        assert_eq!(m.aggregate_type, "Account");
        assert_eq!(m.content_type, "application/json");
        assert_eq!(m.tenant_id, tenant);
        assert!(uuid::Uuid::parse_str(&m.event_id).is_ok());
        assert!(m.timestamp_unix_ms > 0);
    }
    // Flat body, no enum tag.
    assert_eq!(raw[0].payload, br#"{"id":"acct-w","owner":"alice"}"#);
    assert_eq!(raw[1].payload, br#"{"amount":10}"#);
}

#[tokio::test]
async fn foreign_events_load_including_unset_version() {
    let server = spawn_server().await;
    let store: Arc<dyn EventStorePort> = Arc::new(connect(&server.addr).await);
    let tenant = unique_tenant();
    append_raw(
        store.as_ref(),
        &tenant,
        "acct-f",
        &[
            // Extra keys (as the TS SDK writes) are ignored; version 0 is 1.
            (
                "AccountOpened",
                1,
                r#"{"eventType":"AccountOpened","schemaVersion":1,"id":"acct-f","owner":"zed"}"#,
            ),
            ("MoneyDeposited", 0, r#"{"amount": 7}"#),
        ],
    )
    .await;
    let repo = EventStoreRepository::<Account>::new(store, &tenant);
    let loaded = repo.load("acct-f").await.unwrap().unwrap();
    assert_eq!(loaded.aggregate.owner, "zed");
    assert_eq!(loaded.aggregate.balance, 7);
    assert_eq!(loaded.committed_version(), 2);
}

#[tokio::test]
async fn old_events_are_upcast_on_load() {
    let server = spawn_server().await;
    let store: Arc<dyn EventStorePort> = Arc::new(connect(&server.addr).await);
    let tenant = unique_tenant();
    append_raw(
        store.as_ref(),
        &tenant,
        "acct-u",
        &[
            ("AccountOpened", 1, r#"{"id":"acct-u","owner":"amy"}"#),
            // A v1 event under an old name with an old field name.
            ("CashIn", 1, r#"{"amt":3}"#),
        ],
    )
    .await;

    let plain = EventStoreRepository::<Account>::new(store.clone(), &tenant);
    let err = plain.load("acct-u").await.expect_err("no upcaster");
    assert!(
        matches!(&err, Error::UnknownEventType { event_type, .. } if event_type == "CashIn"),
        "{err:?}"
    );

    let upcasters = Upcasters::new().rename("CashIn", 1, "MoneyDeposited", 1, |v| {
        Ok(serde_json::json!({ "amount": v["amt"] }))
    });
    let repo = EventStoreRepository::<Account>::new(store, &tenant).with_upcasters(upcasters);
    let loaded = repo.load("acct-u").await.unwrap().unwrap();
    assert_eq!(loaded.aggregate.balance, 3);
}

#[tokio::test]
async fn undecodable_events_fail_the_load() {
    let server = spawn_server().await;
    let store: Arc<dyn EventStorePort> = Arc::new(connect(&server.addr).await);
    let tenant = unique_tenant();
    append_raw(
        store.as_ref(),
        &tenant,
        "acct-n",
        &[
            ("AccountOpened", 1, r#"{"id":"acct-n","owner":"amy"}"#),
            ("MoneyDeposited", 2, r#"{"amount":1}"#),
        ],
    )
    .await;
    append_raw(
        store.as_ref(),
        &tenant,
        "acct-m",
        &[("AccountOpened", 1, r#"{"id":1}"#)],
    )
    .await;
    let repo = EventStoreRepository::<Account>::new(store, &tenant);
    let err = repo.load("acct-n").await.expect_err("newer version");
    assert!(
        matches!(
            err,
            Error::UnknownEventVersion {
                event_version: 2,
                ..
            }
        ),
        "{err:?}"
    );
    let err = repo.load("acct-m").await.expect_err("bad payload");
    assert!(matches!(err, Error::EventDecode { .. }), "{err:?}");
}

/// Per-command metadata reaches the store on every event and is read back
/// by projections (#376).
#[tokio::test]
async fn command_context_is_stored_on_every_event() {
    let server = spawn_server().await;
    let client = connect(&server.addr).await;
    let tenant = unique_tenant();
    let repo = EventStoreRepository::<Account>::new(Arc::new(client.clone()), &tenant);

    let at = DateTime::<Utc>::from_timestamp_millis(1_700_000_000_123).unwrap();
    let context = EventContext::new()
        .with_actor_id("actor-7".into())
        .with_causation_id("cmd-1".into())
        .with_correlation_id("conv-9".into())
        .with_metadata("x-key".into(), "v".into())
        .with_timestamp(at);
    let mut acct = AggregateInstance::new("acct-ctx".to_string(), Account::default());
    acct.execute_with(open("acct-ctx"), &context).await.unwrap();
    acct.execute_with(AccountCommand::Deposit { amount: 3 }, &context)
        .await
        .unwrap();
    repo.save(&mut acct).await.unwrap();

    let stored = read_raw(&client, &tenant, "acct-ctx").await;
    assert_eq!(stored.len(), 2);
    for data in stored {
        let m = data.meta.clone().unwrap();
        assert_eq!(m.actor_id, "actor-7");
        assert_eq!(m.causation_id, "cmd-1");
        assert_eq!(m.correlation_id, "conv-9");
        assert_eq!(m.headers.get("x-key").map(String::as_str), Some("v"));
        assert_eq!(m.timestamp_unix_ms, 1_700_000_000_123);
        let recorded = RecordedEvent::from_proto(data).unwrap();
        assert_eq!(recorded.actor_id.as_deref(), Some("actor-7"));
        assert_eq!(recorded.causation_id.as_deref(), Some("cmd-1"));
        assert_eq!(recorded.correlation_id.as_deref(), Some("conv-9"));
        assert_eq!(recorded.headers.get("x-key").map(String::as_str), Some("v"));
        assert_eq!(recorded.timestamp_unix_ms, 1_700_000_000_123);
    }
}

/// The context a command is recorded under in the idempotency tests: a
/// command id (deterministic event ids) and a fixed time.
fn command_context(command: u128) -> EventContext {
    EventContext::new()
        .with_command_id(Uuid::from_u128(command))
        .with_timestamp(DateTime::<Utc>::from_timestamp_millis(1_700_000_000_000).unwrap())
}

/// Open + deposit recorded as one command on a fresh instance.
fn opened_by_command(id: &str, command: u128) -> AggregateInstance<Account> {
    let mut acct = AggregateInstance::new(id.to_string(), Account::default());
    acct.add_events_with(
        vec![
            Opened {
                id: id.into(),
                owner: "alice".into(),
            }
            .into(),
            Deposited { amount: 10 }.into(),
        ],
        &command_context(command),
    )
    .unwrap();
    acct
}

/// #427: the instance is lost after an unknown-outcome save (a restart).
/// Re-deciding the same command yields an identical batch under the same
/// caller key, which the store acknowledges without writing it twice.
#[tokio::test]
async fn command_key_survives_losing_the_instance() {
    let server = spawn_server().await;
    let client = connect(&server.addr).await;
    let faulty = Arc::new(FaultyPort::new(client.clone()));
    faulty.lose_acks.store(1, Ordering::SeqCst);
    let tenant = unique_tenant();
    let repo = EventStoreRepository::<Account>::new(faulty.clone(), &tenant)
        .with_retry_policy(RetryPolicy::none());

    let mut first = opened_by_command("acct-k", 1);
    repo.save_with_key(&mut first, "cmd-1")
        .await
        .expect_err("ack lost");
    assert_eq!(stream_len(&client, &tenant, "acct-k").await, 2);
    drop(first);

    let mut again = opened_by_command("acct-k", 1);
    repo.save_with_key(&mut again, "cmd-1")
        .await
        .expect("store recognises the command");
    assert!(!again.has_uncommitted_events());
    assert_eq!(stream_len(&client, &tenant, "acct-k").await, 2);

    // Without a command id the ids are random: the same key is a different
    // batch, refused as a reuse, and nothing is written.
    let mut random = AggregateInstance::new("acct-k".to_string(), Account::default());
    random.execute(open("acct-k")).await.unwrap();
    random
        .execute(AccountCommand::Deposit { amount: 10 })
        .await
        .unwrap();
    let err = repo
        .save_with_key(&mut random, "cmd-1")
        .await
        .expect_err("different batch");
    assert!(err.is_idempotency_key_reused(), "{err:?}");
    assert_eq!(random.uncommitted_count(), 2, "pending kept");
    assert_eq!(stream_len(&client, &tenant, "acct-k").await, 2);
}

/// A re-decided command can repeat deterministic ids, nonces and payloads
/// while changing its metadata (actor, aggregate type), or decide fewer or
/// more events. Each is a different batch, refused under the command's key,
/// never mistaken for a lost ack of the original.
#[tokio::test]
async fn command_key_with_changed_metadata_is_refused() {
    let server = spawn_server().await;
    let client = connect(&server.addr).await;
    let tenant = unique_tenant();
    let repo = EventStoreRepository::<Account>::new(Arc::new(client.clone()), &tenant);

    let mut first = opened_by_command("acct-m2", 1);
    repo.save_with_key(&mut first, "cmd-1").await.unwrap();

    let mut changed = AggregateInstance::new("acct-m2".to_string(), Account::default());
    changed
        .add_events_with(
            vec![
                Opened {
                    id: "acct-m2".into(),
                    owner: "alice".into(),
                }
                .into(),
                Deposited { amount: 10 }.into(),
            ],
            &command_context(1).with_actor_id("someone-else".into()),
        )
        .unwrap();
    assert_eq!(
        changed.uncommitted_events[0].event_id(),
        opened_by_command("acct-m2", 1).uncommitted_events[0].event_id(),
        "same deterministic ids"
    );
    let err = repo
        .save_with_key(&mut changed, "cmd-1")
        .await
        .expect_err("different metadata is a different batch");
    assert!(err.is_idempotency_key_reused(), "{err:?}");
    assert_eq!(changed.uncommitted_count(), 2, "pending kept");
    assert_eq!(stream_len(&client, &tenant, "acct-m2").await, 2);

    // Re-decided with fewer events (a prefix of what ran): refused.
    let mut shorter = AggregateInstance::new("acct-m2".to_string(), Account::default());
    shorter
        .add_events_with(
            vec![Opened {
                id: "acct-m2".into(),
                owner: "alice".into(),
            }
            .into()],
            &command_context(1),
        )
        .unwrap();
    let err = repo
        .save_with_key(&mut shorter, "cmd-1")
        .await
        .expect_err("fewer events is a different batch");
    assert!(err.is_idempotency_key_reused(), "{err:?}");
    assert_eq!(shorter.uncommitted_count(), 1);

    // Re-decided with more events (extends what ran): refused, nothing written.
    let mut longer = opened_by_command("acct-m2", 1);
    longer
        .add_events_with(vec![Deposited { amount: 1 }.into()], &command_context(9))
        .unwrap();
    let err = repo
        .save_with_key(&mut longer, "cmd-1")
        .await
        .expect_err("more events is a different batch");
    assert!(err.is_idempotency_key_reused(), "{err:?}");
    assert_eq!(longer.uncommitted_count(), 3);
    assert_eq!(stream_len(&client, &tenant, "acct-m2").await, 2);

    // Same events, ids and key from another aggregate type: refused.
    let savings_repo = EventStoreRepository::<Savings>::new(Arc::new(client.clone()), &tenant);
    let mut savings = AggregateInstance::new("acct-m2".to_string(), Savings::default());
    savings
        .add_events_with(
            opened_by_command("acct-m2", 1)
                .uncommitted_events
                .into_iter()
                .map(|e| e.event)
                .collect(),
            &command_context(1),
        )
        .unwrap();
    savings_repo
        .save_with_key(&mut savings, "cmd-1")
        .await
        .expect_err("another aggregate type's batch");
    assert_eq!(savings.uncommitted_count(), 2);
    assert_eq!(stream_len(&client, &tenant, "acct-m2").await, 2);
}

/// The continuation of a partly committed keyed batch fails before it
/// commits. The instance is left as the call found it, so retrying under the
/// same key reconciles again and appends the remainder exactly once.
#[tokio::test]
async fn failed_continuation_is_retried_under_the_same_key() {
    let server = spawn_server().await;
    let client = connect(&server.addr).await;
    let faulty = Arc::new(FaultyPort::new(client.clone()));
    faulty.lose_acks.store(1, Ordering::SeqCst);
    let tenant = unique_tenant();
    let repo = EventStoreRepository::<Account>::new(faulty.clone(), &tenant)
        .with_retry_policy(RetryPolicy::none());

    let mut acct = opened_by_command("acct-cont", 1);
    repo.save_with_key(&mut acct, "cmd-1")
        .await
        .expect_err("ack lost");
    assert_eq!(stream_len(&client, &tenant, "acct-cont").await, 2);
    acct.add_events_with(vec![Deposited { amount: 5 }.into()], &command_context(3))
        .unwrap();

    // cmd-1's key does not cover the extended batch: refused, not reconciled.
    let err = repo
        .save_with_key(&mut acct, "cmd-1")
        .await
        .expect_err("extended batch under cmd-1");
    assert!(err.is_idempotency_key_reused(), "{err:?}");
    assert_eq!(acct.uncommitted_count(), 3);

    // Under cmd-3's key the stale revision conflicts, the committed prefix
    // is reconciled, and the continuation is dropped.
    faulty.pass_appends.store(1, Ordering::SeqCst);
    faulty.drop_requests.store(1, Ordering::SeqCst);
    repo.save_with_key(&mut acct, "cmd-3")
        .await
        .expect_err("continuation dropped");
    assert_eq!(acct.uncommitted_count(), 3, "instance as the call found it");
    assert_eq!(stream_len(&client, &tenant, "acct-cont").await, 2);

    repo.save_with_key(&mut acct, "cmd-3")
        .await
        .expect("remainder appended under the continuation key");
    assert!(!acct.has_uncommitted_events());
    assert_eq!(acct.committed_version(), 3);
    assert_eq!(stream_len(&client, &tenant, "acct-cont").await, 3);
    let loaded = repo.load("acct-cont").await.unwrap().unwrap();
    assert_eq!(loaded.aggregate.balance, 15);
}

/// #427: a command that already ran, re-decided on reloaded state, is a
/// different batch (later nonces) and is refused as a key reuse.
#[tokio::test]
async fn command_key_reused_after_reload_is_refused() {
    let server = spawn_server().await;
    let client = connect(&server.addr).await;
    let tenant = unique_tenant();
    let repo = EventStoreRepository::<Account>::new(Arc::new(client.clone()), &tenant);

    let mut acct = opened_by_command("acct-r", 1);
    repo.save_with_key(&mut acct, "cmd-1").await.unwrap();

    let deposit = |acct: &mut AggregateInstance<Account>| {
        acct.add_events_with(vec![Deposited { amount: 5 }.into()], &command_context(2))
            .unwrap();
    };
    let mut loaded = repo.load("acct-r").await.unwrap().unwrap();
    deposit(&mut loaded);
    repo.save_with_key(&mut loaded, "cmd-2").await.unwrap();
    assert_eq!(stream_len(&client, &tenant, "acct-r").await, 3);

    let mut reloaded = repo.load("acct-r").await.unwrap().unwrap();
    deposit(&mut reloaded);
    let err = repo
        .save_with_key(&mut reloaded, "cmd-2")
        .await
        .expect_err("command already ran");
    assert!(err.is_idempotency_key_reused(), "{err:?}");
    assert!(!err.is_concurrency_conflict());
    assert_eq!(stream_len(&client, &tenant, "acct-r").await, 3);

    let err = repo
        .save_with_key(&mut reloaded, "")
        .await
        .expect_err("empty key");
    assert!(matches!(err, Error::InvalidCommand { .. }), "{err:?}");
}

/// Event ids are deterministic per command and index, and stored as given.
#[tokio::test]
async fn command_id_event_ids_are_deterministic_and_stored() {
    let server = spawn_server().await;
    let client = connect(&server.addr).await;
    let tenant = unique_tenant();
    let repo = EventStoreRepository::<Account>::new(Arc::new(client.clone()), &tenant);

    let a = opened_by_command("acct-d", 9);
    let b = opened_by_command("acct-d", 9);
    let ids = |i: &AggregateInstance<Account>| -> Vec<Uuid> {
        i.uncommitted_events.iter().map(|e| e.event_id()).collect()
    };
    assert_eq!(ids(&a), ids(&b));
    assert_ne!(ids(&a)[0], ids(&a)[1]);
    assert_ne!(ids(&a), ids(&opened_by_command("acct-d", 10)));

    let expected = ids(&a);
    let mut a = a;
    repo.save(&mut a).await.unwrap();
    let stored: Vec<Uuid> = read_raw(&client, &tenant, "acct-d")
        .await
        .iter()
        .map(|e| e.meta.as_ref().unwrap().event_id.parse().unwrap())
        .collect();
    assert_eq!(stored, expected);
}
