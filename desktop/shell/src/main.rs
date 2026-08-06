use gpui::{App, Bounds, DisplayId, point, px, size};
use shilpo_assets::Assets;
use shilpo_config::ShellConfig;
use shilpo_services::ShellIpcServer;
use shilpo_shell::{ShellRuntime, bar::geometry::BarGeometry};

#[tokio::main]
async fn main() {
    init_tracing();
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        if args.len() != 2 {
            eprintln!("error: shilpo-shell accepts only one lifecycle flag");
            std::process::exit(2);
        }
        match args[1].as_str() {
            "--help" | "-h" => {
                println!("Usage: shilpo-shell [--help] [--version]");
                println!("\nShilpo Desktop Shell Daemon (GPUI Graphical Process)");
                println!("Supervised by shilpo-shell.service. Use 'shilpo' CLI for control.");
                std::process::exit(0);
            }
            "--version" | "-V" => {
                println!("shilpo-shell {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            _ => {
                eprintln!(
                    "error: unrecognized argument '{}'. shilpo-shell is a daemon process; use 'shilpo' CLI to control the shell.",
                    args[1]
                );
                std::process::exit(2);
            }
        }
    }

    let app = gpui_platform::application().with_assets(Assets);
    let ipc_server = match ShellIpcServer::new() {
        Ok(server) => server,
        Err(shilpo_services::IpcError::AlreadyRunning) => {
            eprintln!("shilpo-shell is already running");
            std::process::exit(2);
        }
        Err(error) => {
            eprintln!("Unable to start secure shell IPC: {error}");
            std::process::exit(1);
        }
    };

    app.run(move |cx| {
        // Initialize Shilpo UI theme & global states
        shilpo_ui::init(cx);
        let config_path = std::env::var("HOME")
            .map(|home| std::path::PathBuf::from(home).join(".config/shilpo/config.toml"))
            .unwrap_or_else(|_| std::path::PathBuf::from(".config/shilpo/config.toml"));
        let config =
            shilpo_config::ShellConfig::load_or_create(&config_path).unwrap_or_else(|error| {
                tracing::error!(error = %error, "failed to load shell config; using defaults");
                shilpo_config::ShellConfig::default()
            });
        shilpo_shell::bar::view::apply_config_theme(&config, None, cx);
        cx.activate(true);
        ShellRuntime::install(cx, ipc_server);
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
            ShellRuntime::sync_displays(cx);
        } else {
            schedule_bar_retry(cx, config);
        }
    });
}

fn schedule_bar_retry(cx: &App, config: ShellConfig) {
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
                ShellRuntime::sync_displays(cx);
                true
            });
            if opened {
                return;
            }
        }

        tracing::warn!("primary display unavailable after 1s; opening degraded bar");
        cx.update(|cx| {
            let geometry = BarGeometry::calculate(
                DisplayId::new(0),
                Bounds::new(point(px(0.), px(0.)), size(px(0.), px(0.))),
                &config.bar,
            );
            ShellRuntime::open_bar(cx, &geometry, false);
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
