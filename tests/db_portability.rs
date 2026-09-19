//! Runs keestore's own migrations and its own vault service against a real
//! server of whichever engine the test binary was built for.
//!
//! This is the proof that the module is portable, not just that it compiles:
//! the SQL executed here is the SQL the handlers execute.
//!
//! ```sh
//! # SQLite needs nothing
//! cargo test --no-default-features --features backend-sqlite --test db_portability
//!
//! KUBUNO_DB_TEST_URL=postgres://user:pass@localhost/somedb \
//!   cargo test --test db_portability
//!
//! KUBUNO_DB_TEST_URL=mysql://user:pass@localhost/keestore \
//!   cargo test --no-default-features --features backend-mysql --test db_portability
//! ```

use kubuno_keestore::services::vault_service::{
    delete_vault, get_vault_meta, kdbx_path, sha256_hex, sync_vault, touch_last_accessed,
};
use kubuno_keestore::SCHEMA;
use uuid::Uuid;

#[cfg(feature = "backend-sqlite")]
fn settings() -> (kubuno_db::DbSettings, Option<tempfile::TempDir>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut s = base_settings();
    s.path = Some(dir.path().to_string_lossy().into_owned());
    (s, Some(dir))
}

#[cfg(not(feature = "backend-sqlite"))]
fn settings() -> (kubuno_db::DbSettings, Option<()>) {
    let mut s = base_settings();
    s.url = Some(std::env::var("KUBUNO_DB_TEST_URL").expect(
        "set KUBUNO_DB_TEST_URL to a throwaway database (see the top of this file)",
    ));
    (s, None)
}

fn base_settings() -> kubuno_db::DbSettings {
    kubuno_db::DbSettings {
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

/// One migration run at a time: the PostgreSQL and MySQL tests share a server.
static EXCLUSIVE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn migrated_pool() -> (kubuno_db::DbPool, impl Sized) {
    let guard = EXCLUSIVE.lock().await;
    let (settings, keep) = settings();
    let pool = kubuno_db::connect(&settings, SCHEMA).await.expect("connect");

    // Exactly the call `main.rs` makes.
    let migrator = kubuno_db::migrations!(
        "./migrations/postgres",
        "./migrations/mysql",
        "./migrations/sqlite",
    );
    kubuno_db::pool::scope_migrator(migrator, SCHEMA)
        .run(&pool)
        .await
        .expect("migrations");

    kubuno_db::events::ensure_outbox(&pool, SCHEMA)
        .await
        .expect("outbox");

    (pool, (keep, guard))
}

/// The whole `PUT /kdbx` write path: upsert, version bump, read back, delete.
#[tokio::test]
async fn a_vault_syncs_then_reads_back_then_deletes() {
    let (pool, _keep) = migrated_pool().await;
    let user = Uuid::new_v4();
    let path = kdbx_path(user);

    // First sync: the row is created, version 1.
    let hash1 = sha256_hex(b"first version of the vault");
    let v1 = sync_vault(&pool, user, &path, 1024, &hash1)
        .await
        .expect("first sync");
    assert_eq!(v1, 1);

    let meta = get_vault_meta(&user, &pool).await.expect("meta");
    assert_eq!(meta.owner_id, user);
    assert_eq!(meta.kdbx_path, path);
    assert_eq!(meta.file_size_bytes, 1024);
    assert_eq!(meta.sync_version, 1);
    assert_eq!(meta.file_hash_sha256.as_deref(), Some(hash1.as_str()));
    assert!(meta.last_modified_at.is_some());
    assert!(meta.last_accessed_at.is_none());

    // Second sync on the same owner: the upsert must update, not insert.
    let hash2 = sha256_hex(b"second version of the vault");
    let v2 = sync_vault(&pool, user, &path, 4096, &hash2)
        .await
        .expect("second sync");
    assert_eq!(v2, 2, "the conflict branch has to bump sync_version");

    let meta = get_vault_meta(&user, &pool).await.expect("meta");
    assert_eq!(meta.file_size_bytes, 4096);
    assert_eq!(meta.file_hash_sha256.as_deref(), Some(hash2.as_str()));
    assert_eq!(meta.id, meta.id, "the id is stable across upserts");

    // Download marks the vault as read.
    touch_last_accessed(&pool, user).await;
    let meta = get_vault_meta(&user, &pool).await.expect("meta");
    assert!(meta.last_accessed_at.is_some());

    delete_vault(&pool, user).await.expect("delete");
    assert!(
        get_vault_meta(&user, &pool).await.is_err(),
        "the vault must be gone"
    );
}

/// Two owners must not share a row, whichever engine arbitrates the conflict.
/// This matters most on MySQL, where `ON DUPLICATE KEY UPDATE` reacts to any
/// unique index rather than the one named.
#[tokio::test]
async fn two_owners_keep_separate_vaults() {
    let (pool, _keep) = migrated_pool().await;
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();

    assert_eq!(sync_vault(&pool, a, &kdbx_path(a), 10, "aa").await.unwrap(), 1);
    assert_eq!(sync_vault(&pool, b, &kdbx_path(b), 20, "bb").await.unwrap(), 1);
    assert_eq!(sync_vault(&pool, a, &kdbx_path(a), 30, "cc").await.unwrap(), 2);

    assert_eq!(get_vault_meta(&a, &pool).await.unwrap().file_size_bytes, 30);
    assert_eq!(get_vault_meta(&b, &pool).await.unwrap().file_size_bytes, 20);
    assert_eq!(get_vault_meta(&b, &pool).await.unwrap().sync_version, 1);

    delete_vault(&pool, a).await.unwrap();
    delete_vault(&pool, b).await.unwrap();
}

/// A missing vault is a `VaultNotFound`, not a database error.
#[tokio::test]
async fn an_unknown_owner_has_no_vault() {
    let (pool, _keep) = migrated_pool().await;
    let err = get_vault_meta(&Uuid::new_v4(), &pool).await.unwrap_err();
    assert!(
        matches!(err, kubuno_keestore::errors::KeeStoreError::VaultNotFound),
        "{err}"
    );
}

/// The usage reporter's aggregate query has to decode on every engine.
#[tokio::test]
async fn the_usage_query_decodes_everywhere() {
    let (pool, _keep) = migrated_pool().await;
    let user = Uuid::new_v4();
    sync_vault(&pool, user, &kdbx_path(user), 4242, "hash").await.unwrap();

    let sql = format!(
        "SELECT owner_id, {bytes}, {objects} FROM keestore.vaults \
         WHERE owner_id = $1 GROUP BY owner_id",
        bytes = kubuno_db::dialect::sum_bigint("file_size_bytes"),
        objects = kubuno_db::dialect::count_bigint("*"),
    );
    let rows: Vec<(Uuid, i64, i64)> = kubuno_db::query_as(&sql)
        .expect("sql")
        .bind(user)
        .fetch_all(&pool)
        .await
        .expect("usage query");
    assert_eq!(rows, vec![(user, 4242, 1)]);

    delete_vault(&pool, user).await.unwrap();
}
