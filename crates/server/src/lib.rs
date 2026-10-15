//! 组装根：读配置、拼路由。不放业务逻辑。

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use axum::{Json, Router, routing::get};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use tower_http::services::{ServeDir, ServeFile};

pub struct Config {
    pub addr: SocketAddr,
    pub web_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl Config {
    /// 从 OWNMIND_* 环境变量读取；值不合法时直接失败。
    pub fn from_env() -> anyhow::Result<Self> {
        let addr = std::env::var("OWNMIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8787".into());
        let web_dir = std::env::var("OWNMIND_WEB_DIR").unwrap_or_else(|_| "web/dist".into());
        let data_dir = std::env::var("OWNMIND_DATA_DIR").unwrap_or_else(|_| "data".into());
        Ok(Self {
            addr: addr.parse()?,
            web_dir: web_dir.into(),
            data_dir: data_dir.into(),
        })
    }
}

pub fn router(config: Config) -> Router {
    let index = config.web_dir.join("index.html");
    let web = ServeDir::new(&config.web_dir).fallback(ServeFile::new(index));
    Router::new()
        .route("/healthz", get(healthz))
        .fallback_service(web)
}

async fn healthz() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

/// 打开（必要时创建）数据目录下的 app.db：WAL、外键开启。
pub async fn open_db(data_dir: &Path) -> anyhow::Result<SqlitePool> {
    std::fs::create_dir_all(data_dir)?;
    let options = SqliteConnectOptions::from_str("sqlite://")?
        .filename(data_dir.join("app.db"))
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true);
    Ok(SqlitePoolOptions::new().connect_with(options).await?)
}

/// 执行全部模块的迁移：合并成一个迁移器，按版本号全局排序，共用 _sqlx_migrations。
pub async fn migrate(pool: &SqlitePool) -> anyhow::Result<()> {
    let modules = [
        &ownmind_auth::api::MIGRATOR,
        &ownmind_conversation::api::MIGRATOR,
    ];
    let mut migrations: Vec<_> = modules.iter().flat_map(|m| m.iter().cloned()).collect();
    migrations.sort_by_key(|m| m.version);
    sqlx::migrate::Migrator::with_migrations(migrations)
        .run(pool)
        .await?;
    Ok(())
}
