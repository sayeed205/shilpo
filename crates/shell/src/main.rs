use gpui::{
    App, Bounds, DisplayId, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
    layer_shell::{KeyboardInteractivity, Layer, LayerShellOptions},
    point, px, size,
};
use shilpo_assets::Assets;
use shilpo_config::ShellConfig;
use shilpo_shell::{
    ShellRuntime,
    bar::{BarView, geometry::BarGeometry},
};

#[tokio::main]
async fn main() {
    init_tracing();
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "msg" {
        if args.len() > 2 {
            let cmd = &args[2];
            let req = match cmd.as_str() {
                "toggle-launcher" => shilpo_services::IpcRequest::ToggleLauncher,
                "toggle-control-center" => shilpo_services::IpcRequest::ToggleControlCenter,
                "reload-config" => shilpo_services::IpcRequest::ReloadConfig,
                "toggle-bar" => shilpo_services::IpcRequest::ToggleBar,
                "focus-workspace" => {
                    if let Some(id_str) = args.get(3) {
                        if let Ok(id) = id_str.parse::<u64>() {
                            shilpo_services::IpcRequest::FocusWorkspace(id)
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
                    let is_dark = args.get(4).map(|s| s == "dark").unwrap_or(true);
                    shilpo_services::IpcRequest::SetTheme {
                        source_argb,
                        is_dark,
                    }
                }
                _ => {
                    eprintln!("Unknown command: {}", cmd);
                    std::process::exit(1);
                }
            };

            match shilpo_services::ShellIpcServer::send_command(req) {
                Ok(resp) => {
                    println!("Success: {}, Message: {}", resp.success, resp.message);
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
        ShellRuntime::install(cx);
        if let Some(display) = cx.primary_display() {
            let geometry = BarGeometry::calculate(display.id(), display.bounds(), &config.bar);
            open_bar(cx, &geometry, true);
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
                let Some(display) = cx.primary_display() else {
                    return false;
                };
                let geometry = BarGeometry::calculate(display.id(), display.bounds(), &config.bar);
                open_bar(cx, &geometry, true)
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
            open_bar(cx, &geometry, false);
        });
    })
    .detach();
}

fn open_bar(cx: &mut App, geometry: &BarGeometry, with_display_geometry: bool) -> bool {
    let options = bar_window_options(geometry, with_display_geometry);
    match cx.open_window(options, BarView::view) {
        Ok(_) => true,
        Err(error) => {
            tracing::error!(error = %error, "failed to open bar window");
            false
        }
    }
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

fn bar_window_options(geometry: &BarGeometry, with_display_geometry: bool) -> WindowOptions {
    WindowOptions {
        titlebar: None,
        window_bounds: with_display_geometry.then_some(WindowBounds::Windowed(geometry.bounds)),
        display_id: with_display_geometry.then_some(geometry.display_id),
        app_id: Some("shilpo-bar".to_string()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "bar".to_string(),
            layer: Layer::Top,
            anchor: geometry.anchor,
            exclusive_zone: Some(geometry.exclusive_zone),
            exclusive_edge: Some(geometry.exclusive_edge),
            margin: geometry.margin,
            keyboard_interactivity: KeyboardInteractivity::None,
        }),
        ..Default::default()
    }
}
