use gpui::{Action, App};
use shilpo_ui::ThemeMode;

pub fn init(cx: &mut App) {
    cx.on_action(|switch: &SwitchThemeMode, cx| {
        let mode = switch.0;
        shilpo_ui::Theme::global_mut(cx).set_mode(mode);
        #[cfg(target_os = "linux")]
        crate::update_desktop_icon_for_theme(cx);
        cx.refresh_windows();
    });
}

#[derive(Action, Clone, PartialEq)]
#[action(namespace = themes, no_json)]
pub(crate) struct SwitchThemeMode(pub(crate) ThemeMode);
