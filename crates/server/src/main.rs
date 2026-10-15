use ownmind_server::{Config, router};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let config = Config::from_env()?;
    let listener = tokio::net::TcpListener::bind(config.addr).await?;
    tracing::info!(addr = %config.addr, "ownmind listening");
    axum::serve(listener, router(config))
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
        })
        .await?;
    Ok(())
}
