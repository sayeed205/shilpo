pub mod adapters;
pub mod client;
pub mod daemon;
pub mod dbus;
pub mod executors;
pub mod persistence;
pub mod portal;

pub use client::ThemeClient;
pub use daemon::{ChangeKind, DaemonCommand, DaemonState, ThemeDaemon, ThemeUpdate};
pub use executors::{AdapterExecutor, PersistenceExecutor, ProjectionStatus};
pub use persistence::{read_state_snapshot, state_file_path, write_state_snapshot};

pub async fn run_theme_daemon() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("shilpo_theme_daemon=info".parse().unwrap()),
        )
        .init();

    tracing::info!("Starting shilpo theme-daemon session daemon");
    let daemon = ThemeDaemon::new().await?;
    daemon.run().await;
    Ok(())
}
