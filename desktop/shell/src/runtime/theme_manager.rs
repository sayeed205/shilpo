use std::path::PathBuf;

use gpui::{App, AppContext};
use shilpo_theme_daemon::{DaemonState, ThemeClient};

use super::{ShellRuntime, surface_manager};

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
                Ok(update) => update.state,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    theme_client_for_task.current_state()
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            };
            cx.update(|cx: &mut App| {
                shilpo_ui::Theme::global_mut(cx).apply_state(&state);
                if cx.has_global::<ShellRuntime>() {
                    ShellRuntime::apply_theme_state(cx, &state);
                }
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

impl ShellRuntime {
    pub(super) fn apply_theme_state(cx: &mut App, state: &DaemonState) {
        let runtime = cx.global_mut::<Self>();
        if let Some(path) = state.wallpaper_path.clone().filter(|path| path.is_file()) {
            runtime.current_wallpaper_path = Some(path);
        }
        let overview_entity = runtime.overview_entity.clone();
        let wallpaper_path = runtime.current_wallpaper_path.clone();
        let cc_handle = runtime.control_center;
        let ov_handle = runtime.overview;

        if let Some(overview) = overview_entity {
            overview.update(cx, |view, cx| {
                view.update_wallpaper_path(wallpaper_path, cx);
            });
        }
        Self::refresh_bars(cx);
        if let Some(cc) = cc_handle {
            let _ = cc.update(cx, |_, _, cx| cx.notify());
        }
        if let Some(ov) = ov_handle {
            let _ = ov.update(cx, |_, _, cx| cx.notify());
        }
        cx.refresh_windows();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn theme_init_applies_daemon_state_and_reports_wallpaper() {
        let cx = gpui::TestAppContext::single();
        cx.update(|cx| shilpo_ui::init_with_source(0xFF006C4C, cx));

        let expected = futures_lite::future::block_on(ThemeClient::new());
        let expected_state = expected.current_state();
        let expected_wallpaper = expected_state
            .wallpaper_path
            .clone()
            .filter(|path| path.is_file());

        let wallpaper = cx.update(init);

        cx.update(|cx| {
            let theme = shilpo_ui::Theme::global(cx);
            assert_eq!(theme.source_argb, expected_state.source_argb);
            assert_eq!(theme.scheme_variant, expected_state.scheme_variant);
        });
        assert_eq!(wallpaper, expected_wallpaper);
    }
}
