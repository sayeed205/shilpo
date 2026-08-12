pub mod actions;
pub mod app_icons;
pub mod bar;
pub mod battery;
pub mod capture;
pub mod error;
pub mod extension_http;
pub mod extension_surface;
pub mod extensions;
pub mod notification;
pub mod osd;
pub mod overview;
pub mod overview_search;
pub mod runtime;
pub mod widgets;
pub mod workspace_miniature;

pub use actions::{
    ActionCategory, ActionDescriptor, ActionId, ActionRegistry, KeybindingManager, Shortcut,
};
pub use bar::BarView;
pub use error::ShellError;
pub use extensions::{
    ContributionDescriptor, ContributionInstance, ContributionSurface, ExtensionCoordinator,
    ExtensionSnapshot,
};
pub use notification::NotificationToastView;
pub use osd::{OsdKind, OsdView};
pub use overview::WorkspaceOverview;
pub use runtime::ShellRuntime;

pub fn run_daemon() {
    init_tracing();

    let shell_bus = futures_lite::future::block_on(async {
        use zbus::fdo::{DBusProxy, RequestNameFlags, RequestNameReply};
        let conn = zbus::Connection::session().await.unwrap_or_else(|error| {
            eprintln!("failed to connect to the session bus: {error}");
            std::process::exit(1);
        });
        let dbus = DBusProxy::new(&conn).await.unwrap_or_else(|error| {
            eprintln!("failed to access the session bus: {error}");
            std::process::exit(1);
        });
        let reply = dbus
            .request_name(
                "org.shilpo.Shell".try_into().unwrap(),
                RequestNameFlags::DoNotQueue.into(),
            )
            .await
            .unwrap_or_else(|error| {
                eprintln!("failed to acquire org.shilpo.Shell: {error}");
                std::process::exit(1);
            });
        if reply != RequestNameReply::PrimaryOwner {
            eprintln!("org.shilpo.Shell is already owned by another process");
            std::process::exit(1);
        }
        conn
    });

    let app = gpui_platform::application()
        .with_assets(crate::Assets)
        .with_quit_mode(gpui::QuitMode::Explicit);
    let ipc_server = match shilpo_services::ShellIpcServer::new() {
        Ok(server) => server,
        Err(shilpo_services::IpcError::AlreadyRunning) => {
            eprintln!("shilpo daemon is already running");
            std::process::exit(2);
        }
        Err(error) => {
            eprintln!("Unable to start secure shell IPC: {error}");
            std::process::exit(1);
        }
    };

    app.run(move |cx| {
        shilpo_ui::init(cx);
        let config_path = std::env::var("HOME")
            .map(|home| std::path::PathBuf::from(home).join(".config/shilpo/config.toml"))
            .unwrap_or_else(|_| std::path::PathBuf::from(".config/shilpo/config.toml"));
        let resolver = crate::config::ConfigResolver::from_primary_path(&config_path);
        let config = match crate::config::migrate_primary_for_startup(&config_path) {
            Ok(outcome) => {
                crate::config::unknown_keys::log_unknown_key_warnings(&outcome.warnings);
                match resolver.resolve_initial() {
                    Ok((snapshot, report)) => {
                        crate::config::unknown_keys::log_unknown_key_warnings(&report.unknown_keys);
                        snapshot.config
                    }
                    Err(error) => {
                        if !config_path.exists() {
                            // First run: create the canonical default file,
                            // never overwrite existing user configuration.
                            let default_config = crate::config::ShellConfig::default();
                            match default_config.save(&config_path) {
                                Ok(()) => default_config,
                                Err(save_error) => {
                                    tracing::error!(
                                        error = %save_error,
                                        "failed to write default config; using defaults"
                                    );
                                    crate::config::ShellConfig::default()
                                }
                            }
                        } else {
                            tracing::error!(
                                error = %error,
                                "failed to load shell config; using defaults"
                            );
                            crate::config::ShellConfig::default()
                        }
                    }
                }
            }
            Err(error) => {
                // Migration failure is fatal to configuration startup. Log it
                // clearly and fall back to defaults without rewriting or
                // downgrading the source document.
                tracing::error!(
                    error = %error,
                    path = %config_path.display(),
                    "config migration failed; using defaults without rewriting the source",
                );
                crate::config::ShellConfig::default()
            }
        };
        bar::view::apply_config_theme(&config, None, cx);
        cx.activate(true);
        ShellRuntime::install(cx, ipc_server, shell_bus);
        cx.spawn(async move |cx| {
            use tokio::signal::unix::{SignalKind, signal};
            let mut sigint = signal(SignalKind::interrupt()).ok();
            let mut sigterm = signal(SignalKind::terminate()).ok();
            tokio::select! {
                _ = async {
                    if let Some(s) = sigint.as_mut() {
                        s.recv().await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                } => {},
                _ = async {
                    if let Some(s) = sigterm.as_mut() {
                        s.recv().await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                } => {},
            }
            tracing::info!("shutdown signal received; stopping shell");
            cx.update(ShellRuntime::shutdown);
        })
        .detach();

        let displays = cx.displays();
        if !displays.is_empty() {
            runtime::ShellSurfaces::request(cx, runtime::SurfaceRequest::SyncDisplays);
        } else {
            schedule_bar_retry(cx);
        }
    });
}

fn schedule_bar_retry(cx: &gpui::App) {
    cx.spawn(async move |cx| {
        for _ in 0..20 {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(50))
                .await;

            let opened = cx.update(|cx| {
                let displays = cx.displays();
                if displays.is_empty() {
                    return false;
                }
                runtime::ShellSurfaces::request(cx, runtime::SurfaceRequest::SyncDisplays);
                true
            });
            if opened {
                return;
            }
        }

        tracing::warn!("primary display unavailable after 1s; opening degraded bar");
        cx.update(|cx| {
            runtime::ShellSurfaces::request(cx, runtime::SurfaceRequest::OpenFallbackBar);
        });
    })
    .detach();
}

fn init_tracing() {
    let default_filter = "warn,shilpo_shell=info,shilpo_services=info";
    let filter = std::env::var("RUST_LOG")
        .ok()
        .filter(|value| !value.is_empty())
        .map(|value| tracing_subscriber::EnvFilter::builder().parse_lossy(value))
        .unwrap_or_else(|| tracing_subscriber::EnvFilter::builder().parse_lossy(default_filter));

    let _ = tracing_subscriber::fmt()
        .compact()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}
