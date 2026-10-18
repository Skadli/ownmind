//! sandboxd 的 HTTP 客户端：令牌放在 Authorization 头里，只在 Compose 内部网络使用。

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// 调用失败：连不上、令牌不对，或 sandboxd 返回了错误。
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("沙盒不可达：{0}")]
    Unreachable(String),
    #[error("{0}")]
    Rejected(String),
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecArgs {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecResult {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub timed_out: bool,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadResult {
    pub content: String,
    pub size: u64,
    pub truncated: bool,
}

/// 沙盒客户端，可随意克隆。
#[derive(Clone)]
pub struct Sandbox {
    base: Arc<str>,
    token: Arc<str>,
    http: reqwest::Client,
}

impl Sandbox {
    /// `base_url` 形如 `http://sandbox:8788`。命令最长 600 秒，所以请求超时放宽到 15 分钟。
    pub fn new(base_url: &str, token: &str) -> Self {
        let http = reqwest::Client::builder()
            // 沙盒在内部网络，绝不经代理。
            .no_proxy()
            .timeout(Duration::from_secs(900))
            .build()
            .expect("构建 HTTP 客户端");
        Self {
            base: base_url.trim_end_matches('/').into(),
            token: token.trim().into(),
            http,
        }
    }

    async fn post<B: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<R, SandboxError> {
        let resp = self
            .http
            .post(format!("{}{path}", self.base))
            .bearer_auth(&*self.token)
            .json(body)
            .send()
            .await
            .map_err(|e| SandboxError::Unreachable(e.to_string()))?;
        let status = resp.status();
        if status.is_success() {
            return resp
                .json()
                .await
                .map_err(|e| SandboxError::Rejected(format!("沙盒返回格式不对：{e}")));
        }
        let text = resp.text().await.unwrap_or_default();
        let msg = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str().map(str::to_owned)))
            .unwrap_or_else(|| format!("沙盒返回 {status}"));
        Err(SandboxError::Rejected(msg))
    }

    pub async fn exec(&self, args: &ExecArgs) -> Result<ExecResult, SandboxError> {
        self.post("/exec", args).await
    }

    pub async fn read(&self, path: &str) -> Result<ReadResult, SandboxError> {
        self.post("/read", &serde_json::json!({ "path": path }))
            .await
    }

    pub async fn write(&self, path: &str, content: &str) -> Result<u64, SandboxError> {
        let v: serde_json::Value = self
            .post(
                "/write",
                &serde_json::json!({ "path": path, "content": content }),
            )
            .await?;
        Ok(v.get("bytes")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0))
    }

    pub async fn edit(
        &self,
        path: &str,
        old_string: &str,
        new_string: &str,
    ) -> Result<(), SandboxError> {
        let _: serde_json::Value = self
            .post(
                "/edit",
                &serde_json::json!({
                    "path": path, "oldString": old_string, "newString": new_string
                }),
            )
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Json;
    use axum::http::HeaderMap;
    use axum::routing::post;

    async fn serve() -> String {
        let app = axum::Router::new()
            .route(
                "/read",
                post(|h: HeaderMap, Json(v): Json<serde_json::Value>| async move {
                    if h.get("authorization").and_then(|v| v.to_str().ok()) != Some("Bearer tok") {
                        return (
                            axum::http::StatusCode::UNAUTHORIZED,
                            Json(serde_json::json!({ "error": "令牌无效" })),
                        );
                    }
                    (
                        axum::http::StatusCode::OK,
                        Json(serde_json::json!({
                            "content": format!("读了 {}", v["path"]), "size": 3, "truncated": false
                        })),
                    )
                }),
            )
            .route(
                "/write",
                post(|| async { Json(serde_json::json!({ "bytes": 5 })) }),
            );
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
        format!("http://{addr}/")
    }

    #[tokio::test]
    async fn sends_token_and_parses_replies() {
        let url = serve().await;
        let ok = Sandbox::new(&url, "tok\n");
        assert!(ok.read("a.txt").await.unwrap().content.contains("a.txt"));
        assert_eq!(ok.write("a", "hello").await.unwrap(), 5);
        let bad = Sandbox::new(&url, "wrong");
        match bad.read("a").await {
            Err(SandboxError::Rejected(m)) => assert_eq!(m, "令牌无效"),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn unreachable_is_reported() {
        let sb = Sandbox::new("http://127.0.0.1:1", "t");
        assert!(matches!(
            sb.read("a").await,
            Err(SandboxError::Unreachable(_))
        ));
    }
}
