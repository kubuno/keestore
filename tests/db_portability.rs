//! Runs keestore's own migrations and its own vault service against a real
//! server of **each** engine, from a single compiled binary — the proof that
//! the engine is a run-time choice, not a build-time one.
//!
//! * SQLite always runs (a temp file, no server).
//! * PostgreSQL runs when `KUBUNO_PG_TEST_URL` points at a throwaway database.
//! * MySQL/MariaDB runs when `KUBUNO_MYSQL_TEST_URL` does.
//!
//! ```sh
//! KUBUNO_PG_TEST_URL=postgres://u:p@localhost:5433/keestore \
//! KUBUNO_MYSQL_TEST_URL=mysql://u:p@localhost:3307/keestore \
//!   cargo test --test db_portability
//! ```
//!
//! The same binary contains all three drivers; each engine's suite is one test.

use kubuno_keestore::services::vault_service::{
    delete_vault, get_vault_meta, kdbx_path, sha256_hex, sync_vault, touch_last_accessed,
};
use kubuno_keestore::SCHEMA;
use uuid::Uuid;

fn base_settings(engine: &str) -> kubuno_db::DbSettings {
    kubuno_db::DbSettings {
        engine: engine.to_string(),
        url: None,
        host: None,
        port: None,
        user: None,
        password: None,
        database: None,
        path: None,
        max_connections: 4,
        min_connections: 0,
        connect_timeout: std::time::Duration::from_secs(10),
        run_migrations: true,
    }
}

/// Migrations only run one at a time: the PostgreSQL and MySQL suites may share
/// a server.
static EXCLUSIVE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn migrated_pool(settings: kubuno_db::DbSettings) -> (kubuno_db::DbPool, impl Sized) {
    let guard = EXCLUSIVE.lock().await;
    let pool = kubuno_db::connect(&settings, SCHEMA).await.expect("connect");

    // Exactly the calls `main.rs` makes.
    kubuno_db::migrations!(
        "./migrations/postgres",
        "./migrations/mysql",
        "./migrations/sqlite",
    )
    .run(&pool, SCHEMA)
    .await
    .expect("migrations");

    kubuno_db::events::ensure_outbox(&pool, SCHEMA)
        .await
        .expect("outbox");

    (pool, guard)
}

/// The whole vault write path, plus the two-owner conflict, the missing-vault
/// case and the usage aggregate — everything the handlers exercise.
async fn full_suite(pool: &kubuno_db::DbPool) {
    // ── the PUT /kdbx path: upsert, version bump, read back, delete ──
    let user = Uuid::new_v4();
    let path = kdbx_path(user);

    let hash1 = sha256_hex(b"first version of the vault");
    let v1 = sync_vault(pool, user, &path, 1024, &hash1).await.expect("first sync");
    assert_eq!(v1, 1);

    let meta = get_vault_meta(&user, pool).await.expect("meta");
    assert_eq!(meta.owner_id, user);
    assert_eq!(meta.kdbx_path, path);
    assert_eq!(meta.file_size_bytes, 1024);
    assert_eq!(meta.sync_version, 1);
    assert_eq!(meta.file_hash_sha256.as_deref(), Some(hash1.as_str()));
    assert!(meta.last_modified_at.is_some());
    assert!(meta.last_accessed_at.is_none());
    let first_id = meta.id;

    // Second sync on the same owner: the upsert must update, not insert.
    let hash2 = sha256_hex(b"second version of the vault");
    let v2 = sync_vault(pool, user, &path, 4096, &hash2).await.expect("second sync");
    assert_eq!(v2, 2, "the conflict branch has to bump sync_version");

    let meta = get_vault_meta(&user, pool).await.expect("meta");
    assert_eq!(meta.file_size_bytes, 4096);
    assert_eq!(meta.file_hash_sha256.as_deref(), Some(hash2.as_str()));
    assert_eq!(meta.id, first_id, "the id is stable across upserts");

    touch_last_accessed(pool, user).await;
    let meta = get_vault_meta(&user, pool).await.expect("meta");
    assert!(meta.last_accessed_at.is_some());

    // ── the usage aggregate has to decode on every engine ──
    let backend = pool.backend();
    let sql = format!(
        "SELECT owner_id, {bytes}, {objects} FROM keestore.vaults \
         WHERE owner_id = $1 GROUP BY owner_id",
        bytes = backend.sum_bigint("file_size_bytes"),
        objects = backend.count_bigint("*"),
    );
    let rows: Vec<(Uuid, i64, i64)> = pool
        .fetch_all_as(&sql, kubuno_db::params![user])
        .await
        .expect("usage query");
    assert_eq!(rows, vec![(user, 4096, 1)]);

    delete_vault(pool, user).await.expect("delete");
    assert!(get_vault_meta(&user, pool).await.is_err(), "the vault must be gone");

    // ── two owners must not share a row (the MySQL ON DUPLICATE KEY caveat) ──
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    assert_eq!(sync_vault(pool, a, &kdbx_path(a), 10, "aa").await.unwrap(), 1);
    assert_eq!(sync_vault(pool, b, &kdbx_path(b), 20, "bb").await.unwrap(), 1);
    assert_eq!(sync_vault(pool, a, &kdbx_path(a), 30, "cc").await.unwrap(), 2);
    assert_eq!(get_vault_meta(&a, pool).await.unwrap().file_size_bytes, 30);
    assert_eq!(get_vault_meta(&b, pool).await.unwrap().file_size_bytes, 20);
    assert_eq!(get_vault_meta(&b, pool).await.unwrap().sync_version, 1);
    delete_vault(pool, a).await.unwrap();
    delete_vault(pool, b).await.unwrap();

    // ── a missing vault is VaultNotFound, not a database error ──
    let err = get_vault_meta(&Uuid::new_v4(), pool).await.unwrap_err();
    assert!(
        matches!(err, kubuno_keestore::errors::KeeStoreError::VaultNotFound),
        "{err}"
    );
}

#[tokio::test]
async fn sqlite_from_the_one_binary() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut s = base_settings("sqlite");
    s.path = Some(dir.path().to_string_lossy().into_owned());
    let (pool, _keep) = migrated_pool(s).await;
    full_suite(&pool).await;
}

#[tokio::test]
async fn postgres_from_the_one_binary() {
    let Ok(url) = std::env::var("KUBUNO_PG_TEST_URL") else {
        eprintln!("skipping: KUBUNO_PG_TEST_URL not set");
        return;
    };
    let mut s = base_settings("postgres");
    s.url = Some(url);
    let (pool, _keep) = migrated_pool(s).await;
    full_suite(&pool).await;
}

#[tokio::test]
async fn mysql_from_the_one_binary() {
    let Ok(url) = std::env::var("KUBUNO_MYSQL_TEST_URL") else {
        eprintln!("skipping: KUBUNO_MYSQL_TEST_URL not set");
        return;
    };
    let mut s = base_settings("mysql");
    s.url = Some(url);
    let (pool, _keep) = migrated_pool(s).await;
    full_suite(&pool).await;
}
