//! sandboxd：沙盒容器里的执行服务（决策 003）。
//!
//! 只在 Compose 内部网络可达；除 /healthz 外所有请求都要 `Authorization: Bearer <令牌>`。
//! 提供在工作目录里执行命令、读写与编辑文件；路径一律限制在工作目录内。

mod exec;
mod fs;

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};

pub use exec::{ExecRequest, ExecResponse, MAX_OUTPUT, MAX_TIMEOUT_SECS};
pub use fs::{EditRequest, ReadRequest, ReadResponse, WriteRequest, WriteResponse};

/// 服务状态：工作目录（已规范化）与令牌。
#[derive(Clone)]
pub struct Sandbox {
    inner: Arc<Inner>,
}

struct Inner {
    root: PathBuf,
    token: String,
}

impl Sandbox {
    /// `root` 必须存在；会被规范化（解析符号链接），作为所有路径的边界。
    pub fn new(root: PathBuf, token: String) -> std::io::Result<Self> {
        let root = root.canonicalize()?;
        Ok(Self {
            inner: Arc::new(Inner { root, token }),
        })
    }

    pub fn root(&self) -> &std::path::Path {
        &self.inner.root
    }
}

/// 接口错误：统一返回 `{"error": "..."}`。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    BadRequest(String),
    #[error("路径越出工作目录")]
    Escape,
    #[error("文件不存在：{0}")]
    NotFound(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Escape => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(serde_json::json!({ "error": self.to_string() })),
        )
            .into_response()
    }
}

pub fn router(sandbox: Sandbox) -> Router {
    let api = Router::new()
        .route("/exec", post(exec::handle))
        .route("/read", post(fs::read))
        .route("/write", post(fs::write))
        .route("/edit", post(fs::edit))
        .route_layer(middleware::from_fn_with_state(
            sandbox.clone(),
            require_token,
        ));
    Router::new()
        .route(
            "/healthz",
            get(|| async { Json(serde_json::json!({ "status": "ok" })) }),
        )
        .merge(api)
        .with_state(sandbox)
}

async fn require_token(State(sb): State<Sandbox>, req: Request, next: Next) -> Response {
    let given = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    if constant_time_eq(given.as_bytes(), sb.inner.token.as_bytes()) {
        next.run(req).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "令牌无效" })),
        )
            .into_response()
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
