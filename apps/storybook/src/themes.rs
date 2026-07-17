use gpui::{Action, App};
use gpui_component::ThemeMode;

pub fn init(cx: &mut App) {
    cx.on_action(|switch: &SwitchThemeMode, cx| {
        let mode = switch.0;
        gpui_component::Theme::change(mode, None, cx);
        cx.refresh_windows();
    });
}

#[derive(Action, Clone, PartialEq)]
#[action(namespace = themes, no_json)]
pub(crate) struct SwitchThemeMode(pub(crate) ThemeMode);
