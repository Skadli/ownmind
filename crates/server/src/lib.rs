//! 组装根：读配置、拼路由。不放业务逻辑。

use std::net::SocketAddr;
use std::path::PathBuf;

use axum::{Json, Router, routing::get};
use serde_json::{Value, json};
use tower_http::services::{ServeDir, ServeFile};

pub struct Config {
    pub addr: SocketAddr,
    pub web_dir: PathBuf,
}

impl Config {
    /// 从 OWNMIND_* 环境变量读取；值不合法时直接失败。
    pub fn from_env() -> anyhow::Result<Self> {
        let addr = std::env::var("OWNMIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8787".into());
        let web_dir = std::env::var("OWNMIND_WEB_DIR").unwrap_or_else(|_| "web/dist".into());
        Ok(Self {
            addr: addr.parse()?,
            web_dir: web_dir.into(),
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
