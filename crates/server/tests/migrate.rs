use std::process::Command;

use sqlx::Row;
use sqlx::sqlite::SqlitePoolOptions;

const TABLES: &[&str] = &[
    "auth_login_session",
    "auth_owner",
    "conv_conversation",
    "conv_message",
    "settings_credential",
    "settings_preference",
];

async fn tables(db: &std::path::Path) -> Vec<String> {
    let url = format!("sqlite://{}", db.display());
    let pool = SqlitePoolOptions::new().connect(&url).await.unwrap();
    let rows = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type = 'table' \
         AND name NOT LIKE 'sqlite_%' AND name != '_sqlx_migrations' ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    rows.iter().map(|r| r.get("name")).collect()
}

fn run_migrate(data_dir: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ownmind"))
        .arg("migrate")
        .env("OWNMIND_DATA_DIR", data_dir)
        .output()
        .unwrap()
}

#[tokio::test]
async fn migrate_command_builds_empty_database_and_is_repeatable() {
    let data = tempfile::tempdir().unwrap();

    let first = run_migrate(data.path());
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let second = run_migrate(data.path());
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );

    assert_eq!(tables(&data.path().join("app.db")).await, TABLES);
}

#[tokio::test]
async fn database_uses_wal_and_foreign_keys() {
    let data = tempfile::tempdir().unwrap();
    let pool = ownmind_server::open_db(data.path()).await.unwrap();

    let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&pool)
        .await
        .unwrap();
    let fk: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((mode.as_str(), fk), ("wal", 1));
}
