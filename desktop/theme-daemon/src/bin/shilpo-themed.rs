use anyhow::Result;
use shilpo_theme_daemon::ThemeDaemon;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("shilpo_theme_daemon=info".parse().unwrap()),
        )
        .init();

    tracing::info!("Starting shilpo-themed session daemon");
    let daemon = ThemeDaemon::new().await?;
    daemon.run().await;
    Ok(())
}
