//! 组装根：读配置、打开数据库、拼路由。不放业务逻辑。

mod http;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use ownmind_auth::api::Auth;
use ownmind_conversation::api::Conversations;
use ownmind_kernel::MasterKey;
use ownmind_runtime::api::Runtime;
use ownmind_settings::api::Settings;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};

pub use http::router;

pub struct Config {
    pub addr: SocketAddr,
    pub web_dir: PathBuf,
    pub data_dir: PathBuf,
    /// 启动 agent 子进程的命令（空格分隔）。
    pub agent_cmd: Vec<String>,
    /// 与沙盒共享的令牌文件（决策 003）；首次启动时生成。
    pub sandbox_token_file: PathBuf,
}

impl Config {
    /// 从 OWNMIND_* 环境变量读取；值不合法时直接失败。
    pub fn from_env() -> anyhow::Result<Self> {
        let addr = std::env::var("OWNMIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8787".into());
        let web_dir = std::env::var("OWNMIND_WEB_DIR").unwrap_or_else(|_| "web/dist".into());
        let data_dir = std::env::var("OWNMIND_DATA_DIR").unwrap_or_else(|_| "data".into());
        let agent_cmd = std::env::var("OWNMIND_AGENT_CMD")
            .unwrap_or_else(|_| "node agent/dist/index.js".into());
        let agent_cmd: Vec<String> = agent_cmd.split_whitespace().map(str::to_owned).collect();
        anyhow::ensure!(!agent_cmd.is_empty(), "OWNMIND_AGENT_CMD 不能为空");
        let sandbox_token_file = std::env::var("OWNMIND_SANDBOX_TOKEN_FILE").map_or_else(
            |_| PathBuf::from(&data_dir).join("sandbox.token"),
            PathBuf::from,
        );
        Ok(Self {
            sandbox_token_file,
            agent_cmd,
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
    pub runtime: Runtime,
}

impl App {
    /// 组装各模块并拉起 agent 子进程。需要在 tokio 运行时里调用。
    pub fn new(db: SqlitePool, key: MasterKey, agent_cmd: Vec<String>) -> Self {
        let conversations = Conversations::new(db.clone());
        let settings = Settings::new(db.clone(), key.clone());
        let runtime = Runtime::spawn(agent_cmd, conversations.clone(), settings.clone());
        Self {
            auth: Auth::new(db, key),
            conversations,
            settings,
            runtime,
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
