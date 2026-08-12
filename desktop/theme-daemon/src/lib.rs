pub mod adapters;
pub mod client;
pub mod daemon;
pub mod dbus;
pub mod executors;
pub mod persistence;
pub mod portal;

pub use client::ThemeClient;
pub use daemon::{
    ChangeKind, DaemonCommand, DaemonState, ThemeDaemon, ThemeDaemonOptions, ThemeUpdate,
};
pub use executors::{AdapterExecutor, PersistenceExecutor, ProjectionStatus};
pub use persistence::{
    read_state_snapshot, read_state_snapshot_from, state_file_path, write_state_snapshot,
};

pub async fn run_theme_daemon(options: ThemeDaemonOptions) -> anyhow::Result<()> {
    let _obs_guard = shilpo_observability::init(
        shilpo_observability::ProcessRole::ThemeDaemon,
        "info,shilpo_theme_daemon=info",
    )
    .map_err(|e| eprintln!("observability warning: {e}"))
    .ok();

    tracing::info!("Starting shilpo theme-daemon session daemon");
    let daemon = ThemeDaemon::with_options(options).await?;
    daemon.run().await;
    Ok(())
}
