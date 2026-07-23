use gpui::{
    App, AppContext as _, Context, Entity, FocusHandle, Focusable, IntoElement, ParentElement, Render,
    Styled, Window,
};
use shilpo_ui::{
    Icon, IconName, app_bar::TopAppBar, button::IconButton, dock::PanelControl, v_flex,
};

use crate::section;

pub struct AppBarStory {
    focus_handle: FocusHandle,
}

impl AppBarStory {
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }
}

impl super::Story for AppBarStory {
    fn title() -> &'static str {
        "TopAppBar (M3 Expressive)"
    }

    fn description() -> &'static str {
        "Header app bars in Small, CenterAligned, Medium, and Large variants."
    }

    fn new_view(window: &mut Window, cx: &mut App) -> Entity<impl Render> {
        Self::view(window, cx)
    }

    fn zoomable() -> Option<PanelControl> {
        None
    }
}

impl Focusable for AppBarStory {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AppBarStory {
    fn render(&mut self, _: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_6()
            .child(
                section("Small TopAppBar")
                    .max_w_xl()
                    .child(
                        TopAppBar::small("ab-s")
                            .navigation_icon(Icon::new(IconName::Menu))
                            .title("Dashboard")
                            .action(IconButton::new("act-1").icon(IconName::Search))
                            .action(IconButton::new("act-2").icon(IconName::EllipsisVertical)),
                    ),
            )
            .child(
                section("CenterAligned TopAppBar")
                    .max_w_xl()
                    .child(
                        TopAppBar::center_aligned("ab-c")
                            .navigation_icon(Icon::new(IconName::ChevronLeft))
                            .title("Settings")
                            .action(IconButton::new("act-3").icon(IconName::Settings)),
                    ),
            )
            .child(
                section("Medium TopAppBar")
                    .max_w_xl()
                    .child(
                        TopAppBar::medium("ab-m")
                            .navigation_icon(Icon::new(IconName::Menu))
                            .title("Inbox")
                            .action(IconButton::new("act-4").icon(IconName::Plus)),
                    ),
            )
    }
}
