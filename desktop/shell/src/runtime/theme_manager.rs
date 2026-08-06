use std::path::PathBuf;

use gpui::{App, AppContext};
use shilpo_theme_daemon::ThemeClient;

use super::{surface_manager, ShellRuntime};

pub struct ThemeManager;

impl ThemeManager {
    pub fn init(cx: &mut App) -> Option<PathBuf> {
        let theme_client = futures_lite::future::block_on(ThemeClient::new());
        let initial_theme_state = theme_client.current_state();
        let initial_wallpaper_path = initial_theme_state
            .wallpaper_path
            .clone()
            .filter(|path| path.is_file());
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
                cx.update(|cx: &mut App| {
                    shilpo_ui::Theme::global_mut(cx).apply_state(&state);
                    if cx.has_global::<ShellRuntime>() {
                        let runtime = cx.global_mut::<ShellRuntime>();
                        if let Some(path) =
                            state.wallpaper_path.clone().filter(|path| path.is_file())
                        {
                            runtime.current_wallpaper_path = Some(path);
                        }
                        let overview_entity = runtime.overview_entity.clone();
                        let wallpaper_path = runtime.current_wallpaper_path.clone();
                        let bar_handles: Vec<_> =
                            runtime.bars.values().map(|(handle, _)| *handle).collect();
                        let cc_handle = runtime.control_center;
                        let ov_handle = runtime.overview;

                        if let Some(overview) = overview_entity {
                            overview.update(cx, |view, cx| {
                                view.update_wallpaper_path(wallpaper_path, cx);
                            });
                        }
                        for handle in bar_handles {
                            let _ = handle.update(cx, |_, _, cx| cx.notify());
                        }
                        if let Some(cc) = cc_handle {
                            let _ = cc.update(cx, |_, _, cx| cx.notify());
                        }
                        if let Some(ov) = ov_handle {
                            let _ = ov.update(cx, |_, _, cx| cx.notify());
                        }
                    }
                    cx.refresh_windows();
                });
            }
        })
        .detach();

        initial_wallpaper_path
    }

    pub fn sync_wallpaper(cx: &mut App, initial_wallpaper_path: Option<PathBuf>) {
        let wallpaper_probe =
            cx.background_spawn(async { surface_manager::query_awww_wallpaper_path() });
        let theme_wallpaper_path = initial_wallpaper_path;
        ThemeClient::spawn_task(async move {
            let client = ThemeClient::new().await;
            if let Some(wallpaper_path) = wallpaper_probe.await {
                let _ = client
                    .set_wallpaper(&wallpaper_path.to_string_lossy())
                    .await;
            } else if let Some(wallpaper_path) = theme_wallpaper_path {
                let _ = client
                    .set_wallpaper(&wallpaper_path.to_string_lossy())
                    .await;
            }
        });
    }
}
