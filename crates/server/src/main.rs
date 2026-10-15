use ownmind_server::{App, Config, migrate, open_db, router};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let config = Config::from_env()?;
    let command = std::env::args().nth(1);

    let pool = open_db(&config.data_dir).await?;
    migrate(&pool).await?;
    match command.as_deref() {
        Some("migrate") => {
            tracing::info!(data_dir = %config.data_dir.display(), "migrations applied");
            Ok(())
        }
        None => serve(config, App::new(pool)).await,
        Some(other) => anyhow::bail!("未知命令：{other}（可用：migrate）"),
    }
}

async fn serve(config: Config, app: App) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(config.addr).await?;
    tracing::info!(addr = %config.addr, "ownmind listening");
    axum::serve(listener, router(app, &config.web_dir))
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
        })
        .await?;
    Ok(())
}
