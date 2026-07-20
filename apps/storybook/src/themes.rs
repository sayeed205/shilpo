use gpui::{Action, App};
use shilpo_ui::ThemeMode;

pub fn init(cx: &mut App) {
    // Follow OS appearance by default while retaining Theme's source color.
    shilpo_ui::Theme::change(ThemeMode::System, None, cx);

    cx.on_action(|switch: &SwitchThemeMode, cx| {
        let mode = switch.0;
        shilpo_ui::Theme::change(mode, None, cx);
        cx.refresh_windows();
    });
}

#[derive(Action, Clone, PartialEq)]
#[action(namespace = themes, no_json)]
pub(crate) struct SwitchThemeMode(pub(crate) ThemeMode);
