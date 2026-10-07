//! Backup and restore drill (#355): `pg_dump` a populated event store,
//! `pg_restore` into a separate fresh container, and prove the restored
//! instance is the same log: event ids, payloads, revisions, replay order,
//! idempotency records, consumer checkpoints, and a projection rebuilt from
//! the restored log. Includes historical event-version fixtures.
//! Run with `make -C event-store recovery-drill`.

mod drill;

use std::path::PathBuf;

use drill::fixtures::historical;
use drill::pg::{DisposablePg, DB, USER};
use drill::projection::{checkpoint_of, state_of, Consumer, Outcome, Stop, Upcasters};
use drill::server::EventStoreProc;
use drill::unique;
use drill::workload::{
    accounts_workload, append_all, assert_exactly_once, deposit, head, read_all,
};
use sqlx::PgPool;
use tonic::Code;

/// Tables a backup must carry, with every column compared.
const TABLES: &[&str] = &[
    "events",
    "aggregates",
    "idempotency",
    "projection_checkpoints",
    "_sqlx_migrations",
    "drill_balances",
    "drill_applied",
];

async fn table_rows(pool: &PgPool, table: &str) -> Vec<String> {
    sqlx::query_scalar(&format!(
        "SELECT to_jsonb(t)::text FROM {table} t ORDER BY 1"
    ))
    .fetch_all(pool)
    .await
    .unwrap_or_else(|e| panic!("read {table}: {e}"))
}

async fn scalar_text(pool: &PgPool, sql: &str) -> String {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

/// Removes the dump directory even if the drill panics.
struct TempDir(PathBuf);
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "recovery drill: needs Docker; run `make -C event-store recovery-drill`"]
async fn pg_dump_restore_to_fresh_instance_preserves_log_and_rebuilds_projection() {
    // ---- Populate the source -------------------------------------------
    let src = DisposablePg::start("backup-src").await;
    let src_pool = src.pool().await;
    let src_es = EventStoreProc::start(&src.url()).await;
    let t_fix = unique("t-fixture");
    let t_load = unique("t-load");
    let (fx_cmds, fx_expected) = historical(&t_fix);
    let (ld_cmds, ld_expected) = accounts_workload(&t_load, 4, 10);
    let fx_acks = append_all(&src_es.endpoint(), &fx_cmds).await;
    append_all(&src_es.endpoint(), &ld_cmds).await;
    // One unkeyed append: not every event has an idempotency record.
    let unkeyed = deposit(&t_load, "acct-000", 12, 1);
    let mut client = src_es.client().await;
    client.append(unkeyed.without_key()).await.unwrap();
    let mut ld_expected = ld_expected;
    ld_expected.get_mut("acct-000").unwrap().balance_minor += 1;
    ld_expected.get_mut("acct-000").unwrap().events_applied += 1;

    // Fixture projection fully caught up; load projection mid-log.
    let fx_consumer = Consumer::new(src_pool.clone(), "balances", &t_fix).await;
    fx_consumer
        .run_to(&src_es.endpoint(), head(&src_pool, &t_fix).await)
        .await;
    assert_eq!(fx_consumer.state().await, fx_expected);
    let ld_consumer = Consumer::new(src_pool.clone(), "balances", &t_load).await;
    let r = ld_consumer
        .run(&src_es.endpoint(), Stop::AfterApplied(25))
        .await;
    assert!(matches!(r.outcome, Outcome::Crashed), "{r:?}");
    let ld_cp = ld_consumer.checkpoint().await;

    let src_fix_log = read_all(&src_es.endpoint(), &t_fix).await;
    let src_load_log = read_all(&src_es.endpoint(), &t_load).await;
    let src_max_g = head(&src_pool, &t_load)
        .await
        .max(head(&src_pool, &t_fix).await);

    // ---- Back up and restore into a separate fresh instance ------------
    let dir = TempDir(std::env::temp_dir().join(unique("esp-drill-dump")));
    std::fs::create_dir_all(&dir.0).unwrap();
    let dump = dir.0.join("eventstore.dump");
    src.exec(&[
        "pg_dump",
        "-U",
        USER,
        "-d",
        DB,
        "--format=custom",
        "--file=/tmp/eventstore.dump",
    ]);
    src.copy_out("/tmp/eventstore.dump", &dump);
    assert!(std::fs::metadata(&dump).unwrap().len() > 0);

    let dst = DisposablePg::start("backup-dst").await;
    assert_ne!(dst.name, src.name);
    let dst_pool = dst.pool().await;
    let fresh: Option<String> = sqlx::query_scalar("SELECT to_regclass('public.events')::text")
        .fetch_one(&dst_pool)
        .await
        .unwrap();
    assert_eq!(fresh, None, "restore target must be empty");
    dst.copy_in(&dump, "/tmp/eventstore.dump");
    dst.exec(&[
        "pg_restore",
        "-U",
        USER,
        "-d",
        DB,
        "--exit-on-error",
        "--single-transaction",
        "--no-owner",
        "/tmp/eventstore.dump",
    ]);

    // ---- Byte-for-byte table comparison ---------------------------------
    for table in TABLES {
        let a = table_rows(&src_pool, table).await;
        let b = table_rows(&dst_pool, table).await;
        assert!(!a.is_empty(), "source {table} is populated");
        assert_eq!(a.len(), b.len(), "{table} row count");
        assert_eq!(a, b, "{table} rows differ after restore");
    }
    let seq = "SELECT last_value::text || '/' || is_called::text FROM events_global_nonce_seq";
    assert_eq!(
        scalar_text(&src_pool, seq).await,
        scalar_text(&dst_pool, seq).await
    );
    let triggers = "SELECT string_agg(tgname, ',' ORDER BY tgname) FROM pg_trigger
                     WHERE tgrelid = 'events'::regclass AND NOT tgisinternal";
    let trg = scalar_text(&dst_pool, triggers).await;
    assert_eq!(scalar_text(&src_pool, triggers).await, trg);
    assert!(trg.contains("trg_events_immutable_update"), "{trg}");
    assert_eq!(
        checkpoint_of(&dst_pool, &format!("balances:{t_load}")).await,
        ld_cp
    );

    // ---- Serve the restored database ------------------------------------
    let migrations_before = table_rows(&dst_pool, "_sqlx_migrations").await;
    let dst_es = EventStoreProc::start(&dst.url()).await;
    assert_eq!(
        table_rows(&dst_pool, "_sqlx_migrations").await,
        migrations_before,
        "startup on a restored database applies no migrations"
    );

    // Replay order and content over the API.
    let dst_fix_log = read_all(&dst_es.endpoint(), &t_fix).await;
    let dst_load_log = read_all(&dst_es.endpoint(), &t_load).await;
    assert_eq!(dst_fix_log, src_fix_log, "fixture log (ReadAll) differs");
    assert_eq!(dst_load_log, src_load_log, "load log (ReadAll) differs");
    for log in [&dst_fix_log, &dst_load_log] {
        let g: Vec<u64> = log
            .iter()
            .map(|e| e.meta.as_ref().unwrap().global_nonce)
            .collect();
        assert!(g.windows(2).all(|w| w[0] < w[1]), "strict global order");
    }
    // Historical versions and exact payload bytes come back unchanged.
    for (cmd, stored) in fx_cmds.iter().zip(&dst_fix_log) {
        let sent = &cmd.req.events[0];
        let (s, m) = (stored.meta.as_ref().unwrap(), sent.meta.as_ref().unwrap());
        assert_eq!(s.event_id, m.event_id);
        assert_eq!(s.event_type, m.event_type);
        assert_eq!(s.event_version, m.event_version);
        assert_eq!(s.headers, m.headers);
        assert_eq!(stored.payload, sent.payload, "fixture payload bytes");
    }
    let versions: std::collections::BTreeSet<_> = dst_fix_log
        .iter()
        .map(|e| {
            let m = e.meta.as_ref().unwrap();
            (m.event_type.clone(), m.event_version)
        })
        .collect();
    assert!(versions.contains(&("AccountOpened".into(), 1)));
    assert!(versions.contains(&("FundsDeposited".into(), 1)));

    // Idempotency records still work after restore.
    let mut dst_client = dst_es.client().await;
    for (cmd, ack) in fx_cmds.iter().zip(&fx_acks) {
        let again = dst_client
            .append(cmd.req.clone())
            .await
            .unwrap()
            .into_inner();
        assert_eq!(&again, ack, "retry of {} after restore", cmd.key());
    }
    let mut altered = fx_cmds[0].req.clone();
    altered.events[0].payload = b"{}".to_vec();
    let refused = dst_client.append(altered).await.unwrap_err();
    assert_eq!(refused.code(), Code::AlreadyExists, "{refused:?}");

    // Append-only enforcement survived the restore.
    let upd = sqlx::query("UPDATE events SET event_type = 'tampered' WHERE tenant_id = $1")
        .bind(&t_fix)
        .execute(&dst_pool)
        .await
        .unwrap_err();
    assert!(upd.to_string().contains("append-only"), "{upd}");

    // ---- Projections on the restored log --------------------------------
    // Rebuild from scratch and compare with the expected state and with the
    // projection state carried by the backup.
    let rebuilt_fx = Consumer::new(dst_pool.clone(), "rebuilt", &t_fix).await;
    rebuilt_fx
        .run_to(&dst_es.endpoint(), head(&dst_pool, &t_fix).await)
        .await;
    rebuilt_fx.assert_complete().await;
    assert_eq!(rebuilt_fx.state().await, fx_expected);
    assert_eq!(
        state_of(&dst_pool, "balances", &t_fix).await,
        fx_expected,
        "restored projection state"
    );

    // A restored checkpoint resumes where it left off, including events
    // appended after the restore.
    let post = deposit(&t_load, "acct-001", 12, 5_000);
    let post_ack = append_all(&dst_es.endpoint(), std::slice::from_ref(&post)).await;
    assert!(
        post_ack[0].last_global_nonce > src_max_g,
        "new appends continue the restored sequence"
    );
    ld_expected.get_mut("acct-001").unwrap().balance_minor += 5_000;
    ld_expected.get_mut("acct-001").unwrap().events_applied += 1;
    let restored_ld = Consumer::new(dst_pool.clone(), "balances", &t_load).await;
    let reports = restored_ld
        .run_to(&dst_es.endpoint(), head(&dst_pool, &t_load).await)
        .await;
    assert_eq!(reports[0].from, ld_cp + 1);
    let applied: usize = reports.iter().map(|r| r.applied).sum();
    let dups: usize = reports.iter().map(|r| r.duplicates_skipped).sum();
    assert_eq!(applied, ld_cmds.len() + 2 - 25, "{reports:?}");
    assert_eq!(dups, 0);
    restored_ld.assert_complete().await;
    assert_eq!(restored_ld.state().await, ld_expected);

    let rebuilt_ld = Consumer::new(dst_pool.clone(), "rebuilt", &t_load).await;
    rebuilt_ld
        .run_to(&dst_es.endpoint(), head(&dst_pool, &t_load).await)
        .await;
    assert_eq!(rebuilt_ld.state().await, ld_expected);

    // The fixture tenant (all keyed) took every command exactly once, even
    // after the post-restore retries above.
    assert_exactly_once(&dst_pool, &t_fix, &fx_cmds).await;
}

/// Domain replay of historical versions needs the consumer's upcasters. A
/// consumer that dropped an "old" upcaster stops at the first event of that
/// version, explicitly, with its checkpoint before it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "recovery drill: needs Docker; run `make -C event-store recovery-drill`"]
async fn historical_versions_replay_only_with_retained_upcasters() {
    let pg = DisposablePg::start("upcasters").await;
    let pool = pg.pool().await;
    let es = EventStoreProc::start(&pg.url()).await;
    let tenant = unique("t-upcast");
    let (cmds, expected) = historical(&tenant);
    append_all(&es.endpoint(), &cmds).await;
    let log = read_all(&es.endpoint(), &tenant).await;
    let first_v1_deposit = log
        .iter()
        .map(|e| e.meta.as_ref().unwrap())
        .find(|m| m.event_type == "FundsDeposited" && m.event_version == 1)
        .unwrap()
        .global_nonce;

    let mut missing = Consumer::new(pool.clone(), "no-v1-deposit", &tenant).await;
    missing.upcasters = Upcasters::full().without("FundsDeposited", 1);
    let r = missing.run(&es.endpoint(), Stop::Never).await;
    match &r.outcome {
        Outcome::HandlerFailed(e) => assert!(
            e.contains(&format!(
                "no upcaster for FundsDeposited v1 at global_nonce {first_v1_deposit}"
            )),
            "{e}"
        ),
        other => panic!("expected a handler failure, got {other:?}"),
    }
    assert!(missing.checkpoint().await < first_v1_deposit);

    let full = Consumer::new(pool.clone(), "full", &tenant).await;
    full.run_to(&es.endpoint(), head(&pool, &tenant).await)
        .await;
    full.assert_complete().await;
    assert_eq!(full.state().await, expected);
}
