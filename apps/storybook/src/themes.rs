use gpui::{Action, App};
use gpui_component::ThemeMode;

pub fn init(cx: &mut App) {
    // Follow OS appearance by default while retaining Theme's source color.
    gpui_component::Theme::change(ThemeMode::System, None, cx);

    cx.on_action(|switch: &SwitchThemeMode, cx| {
        let mode = switch.0;
        gpui_component::Theme::change(mode, None, cx);
        cx.refresh_windows();
    });
}

#[derive(Action, Clone, PartialEq)]
#[action(namespace = themes, no_json)]
pub(crate) struct SwitchThemeMode(pub(crate) ThemeMode);
