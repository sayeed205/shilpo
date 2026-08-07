use std::path::PathBuf;

use gpui::{App, AppContext};
use shilpo_theme_daemon::ThemeClient;

use super::surface_manager::{self, SurfaceManager};

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
            let update = match rx.recv().await {
                Ok(update) => update,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    theme_client_for_task.current_update()
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            };
            let state = update.state;
            cx.update(|cx: &mut App| {
                shilpo_ui::Theme::global_mut(cx).apply_state(&state);
                SurfaceManager::apply_theme_state(cx, &state);
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
