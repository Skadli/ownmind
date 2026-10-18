//! 沙盒容器内的执行服务入口（决策 003）。
//!
//! 环境变量：OWNMIND_SANDBOX_ADDR（默认 0.0.0.0:8788）、OWNMIND_SANDBOX_WORKDIR（默认 /workspace）、
//! OWNMIND_SANDBOX_TOKEN_FILE（默认 /secret/sandbox.token，由主服务首次启动生成）。

use std::path::PathBuf;
use std::time::Duration;

use ownmind_sandboxd::{Sandbox, router};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let env = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.into());
    let addr: std::net::SocketAddr = env("OWNMIND_SANDBOX_ADDR", "0.0.0.0:8788").parse()?;
    let root = PathBuf::from(env("OWNMIND_SANDBOX_WORKDIR", "/workspace"));
    let token_file = PathBuf::from(env("OWNMIND_SANDBOX_TOKEN_FILE", "/secret/sandbox.token"));

    std::fs::create_dir_all(&root)?;
    let token = wait_for_token(&token_file).await;
    let sandbox = Sandbox::new(root, token)?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, root = %sandbox.root().display(), "ownmind-sandboxd listening");
    axum::serve(listener, router(sandbox))
        .with_graceful_shutdown(async {
            if let Ok(mut s) =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            {
                s.recv().await;
            }
        })
        .await?;
    Ok(())
}

/// 令牌由主服务生成：没有就每秒重试，直到出现。
async fn wait_for_token(path: &std::path::Path) -> String {
    let mut warned = false;
    loop {
        match std::fs::read_to_string(path) {
            Ok(s) if !s.trim().is_empty() => return s.trim().to_owned(),
            _ => {
                if !warned {
                    tracing::info!(path = %path.display(), "等待主服务生成令牌");
                    warned = true;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}
