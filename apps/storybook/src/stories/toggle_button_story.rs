use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, IntoElement, ParentElement, Render,
    Styled, Window,
};
use shilpo_ui::{IconName, dock::PanelControl, h_flex, toggle_button::ToggleButton, v_flex};

use crate::section;

pub struct ToggleButtonStory {
    focus_handle: FocusHandle,
    checked_filled: bool,
    checked_elevated: bool,
    checked_outlined: bool,
    checked_tonal: bool,
}

impl ToggleButtonStory {
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            checked_filled: true,
            checked_elevated: false,
            checked_outlined: true,
            checked_tonal: false,
        }
    }

    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }
}

impl super::Story for ToggleButtonStory {
    fn title() -> &'static str {
        "ToggleButton (M3 Expressive)"
    }

    fn description() -> &'static str {
        "Toggle buttons in Filled, Elevated, Outlined, and Tonal variants."
    }

    fn new_view(window: &mut Window, cx: &mut App) -> Entity<impl Render> {
        Self::view(window, cx)
    }

    fn zoomable() -> Option<PanelControl> {
        None
    }
}

impl Focusable for ToggleButtonStory {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ToggleButtonStory {
    fn render(&mut self, _: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().gap_6().child(
            section("ToggleButton Variants (Filled, Elevated, Outlined, Tonal)")
                .max_w_xl()
                .child(
                    h_flex()
                        .gap_3()
                        .child(
                            ToggleButton::filled("tb-1")
                                .label("Filled")
                                .icon(IconName::Check)
                                .checked(self.checked_filled),
                        )
                        .child(
                            ToggleButton::elevated("tb-2")
                                .label("Elevated")
                                .icon(IconName::Star)
                                .checked(self.checked_elevated),
                        )
                        .child(
                            ToggleButton::outlined("tb-3")
                                .label("Outlined")
                                .icon(IconName::Folder)
                                .checked(self.checked_outlined),
                        )
                        .child(
                            ToggleButton::tonal("tb-4")
                                .label("Tonal")
                                .icon(IconName::Add)
                                .checked(self.checked_tonal),
                        ),
                ),
        )
    }
}
