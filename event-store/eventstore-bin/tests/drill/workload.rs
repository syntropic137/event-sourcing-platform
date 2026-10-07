//! Deterministic append workload: "account" streams with stable event ids
//! and idempotency keys, so a retried command is byte-identical to the
//! original request.

use std::collections::BTreeMap;
use std::time::Duration;

use eventstore_proto::gen::event_store_client::EventStoreClient;
use eventstore_proto::gen::{
    AppendRequest, AppendResponse, EventData, EventMetadata, ReadAllRequest, ReadStreamRequest,
};
use sqlx::PgPool;
use tonic::transport::Channel;
use tonic::Code;

use super::projection::AccountState;
use super::server::connect;

pub const AGGREGATE_TYPE: &str = "Account";

/// One logical command: a single append request that must take effect once.
#[derive(Clone, Debug)]
pub struct Cmd {
    pub req: AppendRequest,
}

impl Cmd {
    pub fn key(&self) -> &str {
        &self.req.idempotency_key
    }

    pub fn event_ids(&self) -> Vec<String> {
        self.req
            .events
            .iter()
            .map(|e| e.meta.as_ref().unwrap().event_id.clone())
            .collect()
    }

    pub fn last_nonce(&self) -> u64 {
        self.req
            .events
            .last()
            .unwrap()
            .meta
            .as_ref()
            .unwrap()
            .aggregate_nonce
    }

    /// Same command without an idempotency key.
    pub fn without_key(&self) -> AppendRequest {
        AppendRequest {
            idempotency_key: String::new(),
            ..self.req.clone()
        }
    }
}

pub fn event(
    tenant: &str,
    account: &str,
    nonce: u64,
    event_type: &str,
    event_version: u32,
    payload: Vec<u8>,
) -> EventData {
    EventData {
        meta: Some(EventMetadata {
            event_id: format!("{tenant}/{account}/{nonce}"),
            aggregate_id: account.into(),
            aggregate_type: AGGREGATE_TYPE.into(),
            aggregate_nonce: nonce,
            event_type: event_type.into(),
            event_version,
            content_type: "application/json".into(),
            tenant_id: tenant.into(),
            correlation_id: format!("corr-{account}-{nonce}"),
            timestamp_unix_ms: 1_700_000_000_000 + nonce,
            ..Default::default()
        }),
        payload,
    }
}

fn cmd(tenant: &str, account: &str, events: Vec<EventData>) -> Cmd {
    let first = events[0].meta.as_ref().unwrap().aggregate_nonce;
    let last = events
        .last()
        .unwrap()
        .meta
        .as_ref()
        .unwrap()
        .aggregate_nonce;
    Cmd {
        req: AppendRequest {
            tenant_id: tenant.into(),
            aggregate_id: account.into(),
            aggregate_type: AGGREGATE_TYPE.into(),
            expected_aggregate_nonce: first - 1,
            idempotency_key: format!("cmd/{tenant}/{account}/{first}-{last}"),
            events,
        },
    }
}

pub fn open(tenant: &str, account: &str, owner: &str, currency: &str) -> Cmd {
    let payload = serde_json::json!({ "owner": owner, "currency": currency });
    cmd(
        tenant,
        account,
        vec![event(
            tenant,
            account,
            1,
            "AccountOpened",
            2,
            payload.to_string().into_bytes(),
        )],
    )
}

pub fn deposit(tenant: &str, account: &str, nonce: u64, amount_minor: i64) -> Cmd {
    let payload = serde_json::json!({ "amount_minor": amount_minor, "currency": "USD" });
    cmd(
        tenant,
        account,
        vec![event(
            tenant,
            account,
            nonce,
            "FundsDeposited",
            2,
            payload.to_string().into_bytes(),
        )],
    )
}

/// `accounts` accounts, each opened then given `deposits` deposits, as a
/// round-robin interleaved command list (so global order interleaves streams).
/// Returns the commands and the projection state they must produce.
pub fn accounts_workload(
    tenant: &str,
    accounts: usize,
    deposits: u64,
) -> (Vec<Cmd>, BTreeMap<String, AccountState>) {
    let mut cmds = Vec::new();
    let mut expected = BTreeMap::new();
    let names: Vec<String> = (0..accounts).map(|i| format!("acct-{i:03}")).collect();
    for (i, name) in names.iter().enumerate() {
        cmds.push(open(tenant, name, &format!("owner-{i}"), "USD"));
        expected.insert(
            name.clone(),
            AccountState {
                owner: format!("owner-{i}"),
                currency: "USD".into(),
                balance_minor: 0,
                events_applied: 1,
            },
        );
    }
    for n in 0..deposits {
        for (i, name) in names.iter().enumerate() {
            let amount = 100 + (i as i64) * 7 + (n as i64) * 13;
            cmds.push(deposit(tenant, name, n + 2, amount));
            let st = expected.get_mut(name).unwrap();
            st.balance_minor += amount;
            st.events_applied += 1;
        }
    }
    (cmds, expected)
}

/// Statistics of retrying commands through failures.
#[derive(Debug, Default, Clone)]
pub struct RetryStats {
    /// Could not connect: the request was never sent.
    pub connect_failures: usize,
    /// The append call itself failed (transport error, UNAVAILABLE, timeout):
    /// the outcome of that attempt is unknown.
    pub transport_or_unavailable: usize,
    pub concurrency_retries: usize,
    pub internal: usize,
}

/// Append `cmd` until acknowledged, re-sending the identical request after
/// any transient failure. `endpoint` may go away and come back between
/// attempts. Returns the acknowledgment.
pub async fn append_until_acked(
    endpoint: &str,
    client: &mut Option<EventStoreClient<Channel>>,
    cmd: &Cmd,
    stats: &mut RetryStats,
) -> AppendResponse {
    for _attempt in 0..2000 {
        if client.is_none() {
            match tokio::time::timeout(
                Duration::from_secs(2),
                EventStoreClient::connect(endpoint.to_owned()),
            )
            .await
            {
                Ok(Ok(c)) => *client = Some(c),
                _ => {
                    stats.connect_failures += 1;
                    tokio::time::sleep(Duration::from_millis(25)).await;
                    continue;
                }
            }
        }
        let c = client.as_mut().unwrap();
        let res = tokio::time::timeout(Duration::from_secs(20), c.append(cmd.req.clone())).await;
        match res {
            Ok(Ok(resp)) => return resp.into_inner(),
            Ok(Err(status)) => match status.code() {
                // The original may have committed while this retry waited on
                // its row locks: the retry then fails the optimistic check
                // against the original's own write. Re-sending with the same
                // idempotency key resolves to the recorded result.
                Code::Aborted => stats.concurrency_retries += 1,
                Code::Internal => {
                    stats.internal += 1;
                    *client = None;
                }
                Code::AlreadyExists | Code::InvalidArgument | Code::PermissionDenied => {
                    panic!("non-retryable append failure for {}: {status:?}", cmd.key())
                }
                _ => {
                    stats.transport_or_unavailable += 1;
                    *client = None;
                }
            },
            Err(_) => {
                stats.transport_or_unavailable += 1;
                *client = None;
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("command {} never acknowledged", cmd.key());
}

/// Append each command once, expecting success.
pub async fn append_all(endpoint: &str, cmds: &[Cmd]) -> Vec<AppendResponse> {
    let mut client = connect(endpoint).await;
    let mut acks = Vec::with_capacity(cmds.len());
    for c in cmds {
        let ack = client
            .append(c.req.clone())
            .await
            .unwrap_or_else(|s| panic!("append {}: {s:?}", c.key()))
            .into_inner();
        assert_eq!(ack.last_aggregate_nonce, c.last_nonce());
        acks.push(ack);
    }
    acks
}

/// Read a tenant's whole log over gRPC `ReadAll` (paged), in global order.
pub async fn read_all(endpoint: &str, tenant: &str) -> Vec<EventData> {
    let mut client = connect(endpoint).await;
    let mut out = Vec::new();
    let mut from = 0u64;
    loop {
        let page = client
            .read_all(ReadAllRequest {
                tenant_id: tenant.into(),
                from_global_nonce: from,
                max_count: 97,
                forward: true,
            })
            .await
            .expect("read_all")
            .into_inner();
        out.extend(page.events);
        if page.is_end {
            return out;
        }
        from = page.next_from_global_nonce;
    }
}

/// Read one stream over gRPC.
pub async fn read_stream(endpoint: &str, tenant: &str, aggregate: &str) -> Vec<EventData> {
    let mut client = connect(endpoint).await;
    client
        .read_stream(ReadStreamRequest {
            tenant_id: tenant.into(),
            aggregate_id: aggregate.into(),
            from_aggregate_nonce: 1,
            max_count: 10_000,
            forward: true,
        })
        .await
        .expect("read_stream")
        .into_inner()
        .events
}

/// Highest committed global nonce of a tenant, read directly from Postgres.
pub async fn head(pool: &PgPool, tenant: &str) -> u64 {
    sqlx::query_scalar::<_, Option<i64>>(
        "SELECT max(global_nonce) FROM events WHERE tenant_id = $1",
    )
    .bind(tenant)
    .fetch_one(pool)
    .await
    .unwrap()
    .unwrap_or(0) as u64
}

/// Events with these ids that are committed, read directly from Postgres.
pub async fn committed_count(pool: &PgPool, tenant: &str, event_ids: &[String]) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM events WHERE tenant_id = $1 AND event_id = ANY($2)")
        .bind(tenant)
        .bind(event_ids)
        .fetch_one(pool)
        .await
        .unwrap()
}

pub async fn idempotency_rows(pool: &PgPool, tenant: &str, key: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM idempotency WHERE tenant_id = $1 AND idempotency_key = $2",
    )
    .bind(tenant)
    .bind(key)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Committed global nonce of an event id.
pub async fn global_nonce_of(pool: &PgPool, tenant: &str, event_id: &str) -> Option<u64> {
    sqlx::query_scalar::<_, i64>(
        "SELECT global_nonce FROM events WHERE tenant_id = $1 AND event_id = $2",
    )
    .bind(tenant)
    .bind(event_id)
    .fetch_optional(pool)
    .await
    .unwrap()
    .map(|n| n as u64)
}

/// Assert every command took effect exactly once: one row per event id,
/// contiguous aggregate nonces, one idempotency record per keyed command,
/// and no events other than the commands' own.
pub async fn assert_exactly_once(pool: &PgPool, tenant: &str, cmds: &[Cmd]) {
    let mut expected_ids: Vec<String> = cmds.iter().flat_map(|c| c.event_ids()).collect();
    expected_ids.sort();
    let mut stored: Vec<String> =
        sqlx::query_scalar("SELECT event_id FROM events WHERE tenant_id = $1 ORDER BY event_id")
            .bind(tenant)
            .fetch_all(pool)
            .await
            .unwrap();
    stored.sort();
    assert_eq!(stored.len(), expected_ids.len(), "event count for {tenant}");
    assert_eq!(stored, expected_ids, "event ids for {tenant}");

    let gaps: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM (
             SELECT aggregate_id, count(*) AS n, max(aggregate_nonce) AS m
               FROM events WHERE tenant_id = $1 GROUP BY aggregate_id
         ) s WHERE n <> m",
    )
    .bind(tenant)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(gaps, 0, "aggregate nonces must be contiguous");

    let keyed = cmds.iter().filter(|c| !c.key().is_empty()).count() as i64;
    let idem: i64 = sqlx::query_scalar("SELECT count(*) FROM idempotency WHERE tenant_id = $1")
        .bind(tenant)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(idem, keyed, "one idempotency record per keyed command");
}
