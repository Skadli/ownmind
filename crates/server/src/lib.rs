//! 组装根：读配置、打开数据库、拼路由。不放业务逻辑。

mod http;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use ownmind_auth::api::Auth;
use ownmind_conversation::api::Conversations;
use ownmind_kernel::MasterKey;
use ownmind_settings::api::Settings;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};

pub use http::router;

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

/// 各模块服务的集合，交给 HTTP 层使用。
#[derive(Clone)]
pub struct App {
    pub auth: Auth,
    pub conversations: Conversations,
    pub settings: Settings,
}

impl App {
    pub fn new(db: SqlitePool, key: MasterKey) -> Self {
        Self {
            auth: Auth::new(db.clone()),
            conversations: Conversations::new(db.clone()),
            settings: Settings::new(db, key),
        }
    }
}

/// 数据目录下的主密钥（决策 008），首次启动生成。
pub fn master_key(data_dir: &Path) -> anyhow::Result<MasterKey> {
    std::fs::create_dir_all(data_dir)?;
    Ok(MasterKey::load_or_create(&data_dir.join("master.key"))?)
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
        &ownmind_settings::api::MIGRATOR,
    ];
    let mut migrations: Vec<_> = modules.iter().flat_map(|m| m.iter().cloned()).collect();
    migrations.sort_by_key(|m| m.version);
    sqlx::migrate::Migrator::with_migrations(migrations)
        .run(pool)
        .await?;
    Ok(())
}
