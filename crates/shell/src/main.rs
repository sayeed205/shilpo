use gpui::{App, Bounds, DisplayId, point, px, size};
use shilpo_assets::Assets;
use shilpo_config::ShellConfig;
use shilpo_services::{IpcRequest, IpcResult, ShellIpcServer};
use shilpo_shell::{ShellRuntime, bar::geometry::BarGeometry};

#[tokio::main]
async fn main() {
    init_tracing();
    let args: Vec<String> = std::env::args().collect();

    if args.get(1).is_some_and(|argument| argument == "ext") {
        let exit_code = shilpo_ext::cli::run_cli(&args[2..]);
        if exit_code != 0 {
            std::process::exit(exit_code);
        }
        return;
    }

    if args.len() > 1 && (args[1] == "doctor" || args[1] == "--doctor") {
        let auto_fix = args.iter().any(|a| a == "--fix");
        let doctor = shilpo_shell::DoctorChecker::new();
        let report = doctor.run_diagnostics(auto_fix);
        doctor.print_report(&report);
        return;
    }

    if args.len() > 1 && args[1] == "msg" {
        if args.len() > 2 {
            let cmd = &args[2];
            let req = match cmd.as_str() {
                "get-status" => IpcRequest::GetStatus,
                "toggle-launcher" => IpcRequest::ToggleLauncher,
                "toggle-control-center" => IpcRequest::ToggleControlCenter,
                "reload-config" => IpcRequest::ReloadConfig,
                "toggle-bar" => IpcRequest::ToggleBar,
                "quit" => IpcRequest::Quit,
                "focus-workspace" => {
                    if let Some(id_str) = args.get(3) {
                        if let Ok(id) = id_str.parse::<u64>() {
                            IpcRequest::FocusWorkspace(id)
                        } else {
                            eprintln!("Invalid workspace ID");
                            std::process::exit(1);
                        }
                    } else {
                        eprintln!("Missing workspace ID");
                        std::process::exit(1);
                    }
                }
                "set-theme" => {
                    let source_argb = if let Some(c_str) = args.get(3) {
                        if c_str.starts_with("0x") {
                            u32::from_str_radix(c_str.trim_start_matches("0x"), 16)
                                .unwrap_or(0xff006c4c)
                        } else {
                            c_str.parse::<u32>().unwrap_or(0xff006c4c)
                        }
                    } else {
                        0xff006c4c
                    };
                    let client = shilpo_theme::ThemeClient::new().await;
                    let _ = client.set_custom_seed(source_argb).await;
                    let _ = client
                        .set_color_source(shilpo_theme::ColorSource::Custom)
                        .await;
                    println!("Theme seed set to #{:06X}", source_argb & 0xFFFFFF);
                    return;
                }
                "set-mode" => {
                    let mode_str = args.get(3).map(|s| s.as_str()).unwrap_or("dark");
                    let mode = match mode_str.to_lowercase().as_str() {
                        "light" => shilpo_theme::ThemeMode::Light,
                        "dark" => shilpo_theme::ThemeMode::Dark,
                        _ => shilpo_theme::ThemeMode::System,
                    };
                    let client = shilpo_theme::ThemeClient::new().await;
                    if let Err(e) = client.set_mode(mode).await {
                        eprintln!("Failed to set mode: {}", e);
                        std::process::exit(1);
                    }
                    println!("Theme mode set to {}", mode);
                    return;
                }
                "toggle-mode" => {
                    let client = shilpo_theme::ThemeClient::new().await;
                    if let Err(e) = client.toggle_mode().await {
                        eprintln!("Failed to toggle mode: {}", e);
                        std::process::exit(1);
                    }
                    println!("Toggled theme mode");
                    return;
                }
                "set-wallpaper" => {
                    if let Some(path_str) = args.get(3) {
                        let client = shilpo_theme::ThemeClient::new().await;
                        if let Err(e) = client.set_wallpaper(path_str).await {
                            eprintln!("Failed to set wallpaper: {}", e);
                            std::process::exit(1);
                        }
                        println!("Wallpaper updated to {}", path_str);
                        return;
                    } else {
                        eprintln!("Missing wallpaper file path");
                        std::process::exit(1);
                    }
                }
                "random-wallpaper" => {
                    let client = shilpo_theme::ThemeClient::new().await;
                    if let Err(e) = client.set_random_wallpaper().await {
                        eprintln!("Failed to set random wallpaper: {}", e);
                        std::process::exit(1);
                    }
                    println!("Random wallpaper selected");
                    return;
                }
                "telemetry" => IpcRequest::GetTelemetry,
                _ => {
                    eprintln!("Unknown command: {}", cmd);
                    std::process::exit(1);
                }
            };

            match ShellIpcServer::send_command(req) {
                Ok(resp) => {
                    if !resp.ok {
                        if let Some(error) = resp.error {
                            eprintln!("{}: {}", error.code, error.message);
                        } else {
                            eprintln!("IPC request failed");
                        }
                        std::process::exit(1);
                    }
                    match resp.result {
                        Some(IpcResult::Accepted) => println!("Accepted"),
                        Some(IpcResult::Status(status)) => println!(
                            "running={} bar={:?} launcher_visible={} control_center_visible={} health={:?}",
                            status.running,
                            status.bar,
                            status.launcher_visible,
                            status.control_center_visible,
                            status.health
                        ),
                        Some(IpcResult::Telemetry(health)) => println!(
                            "compositor_connected={} battery={} audio={} network={} notification={} heed={} uptime={}s",
                            health.compositor_connected,
                            health.battery_service_available,
                            health.audio_service_available,
                            health.network_service_available,
                            health.notification_service_available,
                            health.heed_store_available,
                            health.uptime_seconds
                        ),
                        None => println!("Accepted"),
                    }
                }
                Err(e) => {
                    eprintln!("Error sending command: {:?}", e);
                    std::process::exit(1);
                }
            }
        } else {
            eprintln!("Usage: shilpo msg <command> [args]");
            std::process::exit(1);
        }
        return;
    }

    let app = gpui_platform::application().with_assets(Assets);
    let ipc_server = match ShellIpcServer::new() {
        Ok(server) => server,
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
            ShellRuntime::mark_ready(cx);
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
                ShellRuntime::mark_ready(cx);
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
            ShellRuntime::mark_degraded(cx);
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
