use std::path::Path;

use shilpo_theme_daemon::{DaemonState, ThemeClient};
use shilpo_ui::theme::{ColorSource, ThemeMode};

pub struct ThemeAdapter;

impl ThemeAdapter {
    pub async fn get_mode() -> Result<String, (i32, String)> {
        let client = ThemeClient::new().await;
        let state = client.current_state();
        Ok(state.selected_mode.to_string())
    }

    pub async fn set_mode(mode: ThemeMode) -> Result<String, (i32, String)> {
        let client = ThemeClient::new().await;
        client
            .set_mode(mode)
            .await
            .map_err(|e| (3, format!("failed to communicate with theme daemon: {e}")))?;
        Ok(format!("Theme mode set to {mode}"))
    }

    pub async fn toggle_mode() -> Result<String, (i32, String)> {
        let client = ThemeClient::new().await;
        client
            .toggle_mode()
            .await
            .map_err(|e| (3, format!("failed to communicate with theme daemon: {e}")))?;
        let new_mode = client.current_state().selected_mode;
        Ok(format!("Toggled theme mode (now {new_mode})"))
    }

    pub async fn set_seed(color_str: &str) -> Result<String, (i32, String)> {
        let color_argb = if color_str.starts_with("0x") || color_str.starts_with("0X") {
            u32::from_str_radix(
                color_str.trim_start_matches("0x").trim_start_matches("0X"),
                16,
            )
            .map_err(|_| (2, format!("invalid color hex string: '{color_str}'")))?
        } else if color_str.starts_with('#') {
            u32::from_str_radix(color_str.trim_start_matches('#'), 16)
                .map(|val| 0xFF000000 | val)
                .map_err(|_| (2, format!("invalid color hex string: '{color_str}'")))?
        } else {
            color_str
                .parse::<u32>()
                .map_err(|_| (2, format!("invalid color value: '{color_str}'")))?
        };

        let client = ThemeClient::new().await;
        client
            .set_custom_seed(color_argb)
            .await
            .map_err(|e| (3, format!("failed to communicate with theme daemon: {e}")))?;
        client
            .set_color_source(ColorSource::Custom)
            .await
            .map_err(|e| (3, format!("failed to communicate with theme daemon: {e}")))?;

        Ok(format!("Theme seed set to #{:06X}", color_argb & 0xFFFFFF))
    }

    pub async fn set_wallpaper(path: &Path) -> Result<String, (i32, String)> {
        let path_str = path.to_string_lossy();
        let client = ThemeClient::new().await;
        client
            .set_wallpaper(&path_str)
            .await
            .map_err(|e| (3, format!("failed to communicate with theme daemon: {e}")))?;
        Ok(format!("Wallpaper updated to {path_str}"))
    }

    pub async fn get_wallpaper() -> Result<DaemonState, (i32, String)> {
        let client = ThemeClient::new().await;
        Ok(client.current_state())
    }

    pub async fn random_wallpaper() -> Result<String, (i32, String)> {
        let client = ThemeClient::new().await;
        client
            .set_random_wallpaper()
            .await
            .map_err(|e| (3, format!("failed to communicate with theme daemon: {e}")))?;
        Ok("Random wallpaper selected".into())
    }
}
