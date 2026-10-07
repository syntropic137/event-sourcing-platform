//! `PostgresProjectionStore`: read model and checkpoint commit in one
//! transaction. Requires `--features postgres` and `TEST_DATABASE_URL`;
//! skipped otherwise.
#![cfg(feature = "postgres")]

mod common;

use std::sync::Arc;

use async_trait::async_trait;
use common::{connect, spawn_server, unique_tenant};
use event_sourcing_rust::client::{proto, EventStoreClient, EventStorePort};
use event_sourcing_rust::error::{Error, Result};
use event_sourcing_rust::projection::{
    CheckpointKey, CheckpointedProjection, DispatchContext, PostgresProjectionStore,
    ProjectionRunner, ProjectionStore, RecordedEvent,
};
use sqlx::{PgPool, Postgres, Row, Transaction};

async fn append(client: &EventStoreClient, tenant: &str, nonce: u64, amount: i64) -> u64 {
    client
        .append(proto::AppendRequest {
            tenant_id: tenant.into(),
            aggregate_id: "acct".into(),
            aggregate_type: "Account".into(),
            expected_aggregate_nonce: nonce - 1,
            idempotency_key: String::new(),
            events: vec![proto::EventData {
                meta: Some(proto::EventMetadata {
                    event_id: uuid::Uuid::new_v4().to_string(),
                    aggregate_nonce: nonce,
                    event_type: "Deposited".into(),
                    event_version: 1,
                    content_type: "application/json".into(),
                    tenant_id: tenant.into(),
                    ..Default::default()
                }),
                payload: format!("{{\"amount\":{amount}}}").into_bytes(),
            }],
        })
        .await
        .unwrap()
        .last_global_nonce
}

struct SqlBalances {
    version: u32,
    fail_at: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Deposited {
    amount: i64,
}

impl event_sourcing_rust::event::EventSchema for Deposited {
    const EVENT_TYPE: &'static str = "Deposited";
}

#[async_trait]
impl CheckpointedProjection<PostgresProjectionStore> for SqlBalances {
    fn name(&self) -> &str {
        "sql-balances"
    }
    fn version(&self) -> u32 {
        self.version
    }
    async fn handle(
        &mut self,
        tx: &mut Transaction<'static, Postgres>,
        event: &RecordedEvent,
        _ctx: &DispatchContext,
    ) -> Result<()> {
        let Deposited { amount } = event.decode()?;
        sqlx::query(
            "INSERT INTO rs_sdk_test_balances (tenant, version, account, balance) \
             VALUES ($1, $2, $3, $4) ON CONFLICT (tenant, version, account) \
             DO UPDATE SET balance = rs_sdk_test_balances.balance + EXCLUDED.balance",
        )
        .bind(&event.tenant_id)
        .bind(self.version as i32)
        .bind(&event.aggregate_id)
        .bind(amount)
        .execute(&mut **tx)
        .await
        .map_err(|e| Error::Repository(e.into()))?;
        if self.fail_at == Some(event.global_nonce) {
            return Err(Error::domain("handler failure after write"));
        }
        Ok(())
    }
    async fn reset(
        &mut self,
        tx: &mut Transaction<'static, Postgres>,
        key: &CheckpointKey,
    ) -> Result<()> {
        sqlx::query("DELETE FROM rs_sdk_test_balances WHERE tenant = $1 AND version = $2")
            .bind(&key.tenant_id)
            .bind(key.projection_version as i32)
            .execute(&mut **tx)
            .await
            .map_err(|e| Error::Repository(e.into()))?;
        Ok(())
    }
}

async fn balance(pool: &PgPool, tenant: &str, version: i32) -> i64 {
    sqlx::query("SELECT balance FROM rs_sdk_test_balances WHERE tenant = $1 AND version = $2")
        .bind(tenant)
        .bind(version)
        .fetch_optional(pool)
        .await
        .unwrap()
        .map(|r| r.get::<i64, _>("balance"))
        .unwrap_or(0)
}

#[tokio::test]
async fn postgres_store_commits_read_model_and_checkpoint_atomically() {
    let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
        // Locally the Postgres test is opt-in; under CI a missing database
        // must fail, never pass silently.
        assert!(
            std::env::var_os("CI").is_none(),
            "TEST_DATABASE_URL must be set when running postgres-feature tests in CI"
        );
        eprintln!("skipping: TEST_DATABASE_URL not set");
        return;
    };
    let pool = PgPool::connect(&url).await.unwrap();
    let store = Arc::new(PostgresProjectionStore::new(pool.clone()));
    store.migrate().await.unwrap();
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS rs_sdk_test_balances (tenant TEXT, version INTEGER, \
         account TEXT, balance BIGINT NOT NULL, PRIMARY KEY (tenant, version, account))",
    )
    .execute(&pool)
    .await
    .unwrap();

    let server = spawn_server().await;
    let client = connect(&server.addr).await;
    let port: Arc<dyn EventStorePort> = Arc::new(client.clone());
    let tenant = unique_tenant();
    let g1 = append(&client, &tenant, 1, 1).await;
    let g2 = append(&client, &tenant, 2, 2).await;
    let g3 = append(&client, &tenant, 3, 4).await;

    // Handler fails after writing: the write and the checkpoint roll back.
    let mut runner = ProjectionRunner::new(
        port.clone(),
        store.clone(),
        SqlBalances {
            version: 1,
            fail_at: Some(g2),
        },
        &tenant,
    );
    let key = runner.key().clone();
    assert!(matches!(
        runner.catch_up().await,
        Err(Error::ProjectionFailed { global_nonce, .. }) if global_nonce == g2
    ));
    assert_eq!(store.load_checkpoint(&key).await.unwrap(), Some(g1));
    assert_eq!(balance(&pool, &tenant, 1).await, 1);

    // Restart: resumes at the failed event, applies each event once.
    let mut restarted = ProjectionRunner::new(
        port.clone(),
        store.clone(),
        SqlBalances {
            version: 1,
            fail_at: None,
        },
        &tenant,
    );
    assert_eq!(restarted.catch_up().await.unwrap(), g3);
    assert_eq!(balance(&pool, &tenant, 1).await, 7);

    // A second writer on the same key is fenced and its writes roll back.
    let mut tx = store.begin(&key).await.unwrap();
    sqlx::query("UPDATE rs_sdk_test_balances SET balance = -1 WHERE tenant = $1")
        .bind(&tenant)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(store.commit(tx, &key, g3).await.is_err());
    assert_eq!(balance(&pool, &tenant, 1).await, 7);

    // Rebuild equivalence, and a new version builds independently.
    restarted.rebuild().await.unwrap();
    assert_eq!(store.load_checkpoint(&key).await.unwrap(), None);
    assert_eq!(restarted.catch_up().await.unwrap(), g3);
    assert_eq!(balance(&pool, &tenant, 1).await, 7);

    let mut v2 = ProjectionRunner::new(
        port.clone(),
        store.clone(),
        SqlBalances {
            version: 2,
            fail_at: None,
        },
        &tenant,
    );
    v2.catch_up().await.unwrap();
    assert_eq!(balance(&pool, &tenant, 2).await, 7);
    assert_eq!(store.load_checkpoint(&key).await.unwrap(), Some(g3));
}
