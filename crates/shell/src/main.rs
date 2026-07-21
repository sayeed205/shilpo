use gpui::{
    Bounds, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
    layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
    point, px, size,
};
use shilpo_assets::Assets;
use shilpo_shell::bar::BarView;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "msg" {
        if args.len() > 2 {
            let cmd = &args[2];
            let req = match cmd.as_str() {
                "toggle-launcher" => shilpo_services::IpcRequest::ToggleLauncher,
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
        shilpo_ui::Theme::change(shilpo_ui::ThemeMode::System, None, cx);
        cx.activate(true);

        let window_size = size(px(1920.), px(48.));
        let options = WindowOptions {
            titlebar: None,
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.), px(0.)),
                size: window_size,
            })),
            app_id: Some("shilpo-bar".to_string()),
            window_background: WindowBackgroundAppearance::Transparent,
            kind: WindowKind::LayerShell(LayerShellOptions {
                namespace: "bar".to_string(),
                layer: Layer::Top,
                anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
                exclusive_zone: Some(px(40.)),
                margin: None,
                keyboard_interactivity: KeyboardInteractivity::None,
                ..Default::default()
            }),
            ..Default::default()
        };

        cx.open_window(options, BarView::view)
            .expect("failed to open status bar window");
    });
}
