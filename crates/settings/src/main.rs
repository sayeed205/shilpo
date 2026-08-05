use gpui::{App, Bounds, WindowBounds, WindowKind, WindowOptions, point, px, size};
use shilpo_assets::Assets;
use shilpo_settings::SettingsView;
use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

fn single_instance_socket_path() -> PathBuf {
    dirs::runtime_dir()
        .unwrap_or_else(shilpo_config::cache_dir)
        .join("shilpo-settings.sock")
}

fn focus_settings_window_in_compositor() {
    if let Ok(mut socket) = niri_ipc::socket::Socket::connect()
        && let Ok(Ok(niri_ipc::Response::Windows(windows))) =
            socket.send(niri_ipc::Request::Windows)
        && let Some(settings_win) = windows.iter().find(|w| {
            w.app_id.as_deref() == Some("org.shilpo.settings")
                || w.app_id.as_deref() == Some("shilpo-settings")
        })
    {
        let _ = socket.send(niri_ipc::Request::Action(niri_ipc::Action::FocusWindow {
            id: settings_win.id,
        }));
    }
}

fn try_activate_existing_instance() -> bool {
    let socket_path = single_instance_socket_path();
    if let Ok(mut stream) = UnixStream::connect(&socket_path) {
        let _ = stream.write_all(b"focus\n");
        focus_settings_window_in_compositor();
        return true;
    }
    false
}

#[tokio::main]
async fn main() {
    if try_activate_existing_instance() {
        println!("Shilpo Settings is already running. Focused existing window.");
        return;
    }

    let socket_path = single_instance_socket_path();
    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path).ok();

    let theme_client = shilpo_theme::ThemeClient::new().await;
    let initial_theme_state = theme_client.current_state();

    let app = gpui_platform::application().with_assets(Assets);

    app.run(move |cx: &mut App| {
        shilpo_ui::init(cx);

        shilpo_ui::Theme::global_mut(cx).apply_state(&initial_theme_state);

        let mut rx = theme_client.subscribe();
        let theme_client_for_task = theme_client.clone();
        cx.spawn(async move |cx| {
            loop {
                let state = match rx.recv().await {
                    Ok(state) => state,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        theme_client_for_task.current_state()
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                };
                cx.update(|cx| {
                    shilpo_ui::Theme::global_mut(cx).apply_state(&state);
                    cx.refresh_windows();
                });
            }
        })
        .detach();

        if let Some(listener) = listener {
            listener.set_nonblocking(true).ok();
            cx.spawn(async move |cx| {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    if let Ok((_stream, _)) = listener.accept() {
                        focus_settings_window_in_compositor();
                        cx.update(|cx| {
                            cx.activate(true);
                            cx.refresh_windows();
                        });
                    }
                }
            })
            .detach();
        }

        cx.activate(true);

        let display_bounds = cx
            .primary_display()
            .map(|d| d.bounds())
            .unwrap_or_else(|| Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.))));

        let width = px(900.);
        let height = px(640.);
        let origin = point(
            display_bounds.origin.x + (display_bounds.size.width - width) / 2.0,
            display_bounds.origin.y + (display_bounds.size.height - height) / 2.0,
        );

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                origin,
                size(width, height),
            ))),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("Settings".into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            kind: WindowKind::Normal,
            display_id: cx.primary_display().map(|d| d.id()),
            window_background: gpui::WindowBackgroundAppearance::Opaque,
            app_id: Some("org.shilpo.settings".into()),
            ..Default::default()
        };

        if let Err(err) = cx.open_window(options, SettingsView::view) {
            eprintln!("Failed to open Settings window: {}", err);
        }
    });
}
