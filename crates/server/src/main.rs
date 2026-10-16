use ownmind_server::{App, Config, master_key, migrate, open_db, router};
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
        None => {
            let key = master_key(&config.data_dir)?;
            let app = App::new(pool, key, config.agent_cmd.clone());
            serve(config, app).await
        }
        Some(other) => anyhow::bail!("未知命令：{other}（可用：migrate）"),
    }
}

async fn serve(config: Config, app: App) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(config.addr).await?;
    tracing::info!(addr = %config.addr, "ownmind listening");
    axum::serve(listener, router(app, &config.web_dir))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// Ctrl-C 或 SIGTERM（docker stop）都触发优雅退出。
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {}
        () = term => {}
    }
    tracing::info!("shutting down");
}
