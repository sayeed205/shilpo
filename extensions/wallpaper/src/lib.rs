#![cfg_attr(not(any(target_arch = "wasm32", test)), allow(dead_code))]

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct WallpaperExtensionSettings {
    pub wallpaper_paths: Vec<String>,
    pub slideshow_enabled: bool,
    pub slideshow_interval_seconds: u32,
    pub workspace_map: std::collections::BTreeMap<String, String>,
}

impl Default for WallpaperExtensionSettings {
    fn default() -> Self {
        Self {
            wallpaper_paths: vec!["~/Pictures/Wallpapers/example.png".into()],
            slideshow_enabled: false,
            slideshow_interval_seconds: 300,
            workspace_map: std::collections::BTreeMap::new(),
        }
    }
}

pub struct WallpaperExtensionState {
    pub settings: WallpaperExtensionSettings,
    pub current_index: usize,
    pub curated_wallpapers: Vec<String>,
}

impl Default for WallpaperExtensionState {
    fn default() -> Self {
        Self {
            settings: WallpaperExtensionSettings::default(),
            current_index: 0,
            curated_wallpapers: vec!["~/Pictures/Wallpapers/example.png".into()],
        }
    }
}

impl WallpaperExtensionState {
    pub fn next_wallpaper_path(&mut self) -> String {
        if self.curated_wallpapers.is_empty() {
            return String::new();
        }
        let path = self.curated_wallpapers[self.current_index % self.curated_wallpapers.len()].clone();
        self.current_index = (self.current_index + 1) % self.curated_wallpapers.len();
        path
    }

    pub fn wallpaper_for_workspace(&self, workspace_id: &str) -> String {
        if self.curated_wallpapers.is_empty() {
            return String::new();
        }
        let hash = workspace_id.bytes().fold(0usize, |acc, b| acc.wrapping_add(b as usize));
        self.curated_wallpapers[hash % self.curated_wallpapers.len()].clone()
    }

    fn apply_settings(&mut self, settings: WallpaperExtensionSettings) {
        self.settings = settings;
        self.curated_wallpapers = self.settings.wallpaper_paths.clone();
        self.current_index = 0;
    }
}

#[cfg(target_arch = "wasm32")]
mod guest {
    use super::{WallpaperExtensionSettings, WallpaperExtensionState};
    use std::cell::RefCell;

    wit_bindgen::generate!({
        path: "../../core/ext-api/wit",
        world: "extension",
    });

    use shilpo::extension::{events, types, view, wallpaper};

    fn settings_from_value(value: types::DataValue) -> Option<WallpaperExtensionSettings> {
        let bytes = match value {
            types::DataValue::BytesValue(bytes) => bytes,
            types::DataValue::TextValue(text) => text.into_bytes(),
            _ => return None,
        };
        serde_json::from_slice(&bytes).ok()
    }

    thread_local! {
        static STATE: RefCell<WallpaperExtensionState> = RefCell::new(WallpaperExtensionState::default());
    }

    struct WallpaperExtension;

    impl Guest for WallpaperExtension {
        fn activate(_activation: types::Activation) -> Result<(), types::Error> {
            Ok(())
        }

        fn deactivate(_reason: types::DeactivateReason) -> Result<(), types::Error> {
            Ok(())
        }

        fn on_event(event: events::ExtensionEvent) -> Result<(), types::Error> {
            match event {
                events::ExtensionEvent::ContributionSettingsChanged(settings) => {
                    if settings.contribution_id == "settings" {
                        if let Some(value) = settings_from_value(settings.settings) {
                            STATE.with(|state| state.borrow_mut().apply_settings(value));
                        }
                    }
                }
                events::ExtensionEvent::WallpaperRequest(req) => {
                    let (path, source) = STATE.with(|s| {
                        let mut state = s.borrow_mut();
                        if state.curated_wallpapers.is_empty() {
                            return (String::new(), wallpaper::WallpaperSource::LocalFile);
                        }
                        match req.reason {
                            wallpaper::WallpaperRequestReason::WorkspaceChanged => {
                                let ws_id = match &req.target {
                                    wallpaper::WallpaperTarget::Workspace(w) => w.workspace_id.as_str(),
                                    wallpaper::WallpaperTarget::Global => "0",
                                };
                                let mapped = state.settings.workspace_map.get(ws_id).cloned();
                                (mapped.unwrap_or_else(|| state.wallpaper_for_workspace(ws_id)), wallpaper::WallpaperSource::LocalFile)
                            }
                            _ if req.reason == wallpaper::WallpaperRequestReason::SlideshowTick
                                && !state.settings.slideshow_enabled => (String::new(), wallpaper::WallpaperSource::LocalFile),
                            _ => (state.next_wallpaper_path(), wallpaper::WallpaperSource::LocalFile),
                        }
                    });

                    if path.is_empty() {
                        return Ok(());
                    }

                    let set_req = wallpaper::WallpaperSetRequest {
                        path,
                        source,
                        request_id: Some(req.request_id),
                        target: Some(req.target),
                    };
                    let _ = wallpaper::set(&set_req);
                }
                events::ExtensionEvent::WallpaperResult(_res) => {
                    // Host notified outcome of applied wallpaper
                }
                events::ExtensionEvent::WorkspaceChanged(_ws) => {
                    // Workspace change notification
                }
                _ => {}
            }
            Ok(())
        }

        fn view(_contribution_id: String) -> Result<Option<view::ViewTree>, types::Error> {
            Ok(None)
        }
    }

    export!(WallpaperExtension);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_rotates_curated_wallpapers() {
        let mut state = WallpaperExtensionState::default();
        state.curated_wallpapers = vec!["/one.png".into(), "/two.png".into()];
        let first = state.next_wallpaper_path();
        let second = state.next_wallpaper_path();
        assert_ne!(first, second);
    }

    #[test]
    fn state_deterministically_selects_workspace_wallpaper() {
        let mut state = WallpaperExtensionState::default();
        state.curated_wallpapers = vec!["/one.png".into(), "/two.png".into()];
        let ws1_a = state.wallpaper_for_workspace("1");
        let ws1_b = state.wallpaper_for_workspace("1");
        assert_eq!(ws1_a, ws1_b);
    }
}
