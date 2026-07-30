use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, IntoElement, ParentElement, Render,
    Styled, Window,
};
use shilpo_ui::{
    IconName, button::IconButton, dock::PanelControl, floating_toolbar::FloatingToolbar, h_flex,
    v_flex,
};

use crate::section;

pub struct FloatingToolbarStory {
    focus_handle: FocusHandle,
}

impl FloatingToolbarStory {
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }
}

impl super::Story for FloatingToolbarStory {
    fn title() -> &'static str {
        "FloatingToolbar (M3 Expressive)"
    }

    fn description() -> &'static str {
        "Context-sensitive floating toolbars in horizontal and vertical pill shapes."
    }

    fn new_view(window: &mut Window, cx: &mut App) -> Entity<impl Render> {
        Self::view(window, cx)
    }

    fn zoomable() -> Option<PanelControl> {
        None
    }
}

impl Focusable for FloatingToolbarStory {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for FloatingToolbarStory {
    fn render(&mut self, _: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_6()
            .child(
                section("Horizontal Floating Toolbar (Standard Pill)")
                    .max_w_lg()
                    .child(
                        h_flex()
                            .gap_3()
                            .items_center()
                            .child(
                                FloatingToolbar::horizontal("ft-h-1")
                                    .child(IconButton::new("ft-btn-1").icon(IconName::Search))
                                    .child(IconButton::new("ft-btn-2").icon(IconName::Close))
                                    .child(IconButton::new("ft-btn-3").icon(IconName::Folder))
                                    .child(IconButton::new("ft-btn-4").icon(IconName::MoreVert)),
                            )
                            .child(IconButton::new("ft-fab-1").icon(IconName::Add)),
                    ),
            )
            .child(
                section("Horizontal Floating Toolbar (Vibrant Accent)")
                    .max_w_lg()
                    .child(
                        h_flex().gap_3().items_center().child(
                            FloatingToolbar::horizontal("ft-h-2")
                                .vibrant(true)
                                .child(IconButton::new("ft-btn-5").icon(IconName::CalendarToday))
                                .child(IconButton::new("ft-btn-6").icon(IconName::Person))
                                .child(IconButton::new("ft-btn-7").icon(IconName::Close)),
                        ),
                    ),
            )
            .child(
                section("Vertical Floating Toolbar").max_w_lg().child(
                    v_flex().child(
                        FloatingToolbar::vertical("ft-v-1")
                            .child(IconButton::new("ft-vbtn-1").icon(IconName::Add))
                            .child(IconButton::new("ft-vbtn-2").icon(IconName::Folder))
                            .child(IconButton::new("ft-vbtn-3").icon(IconName::Star)),
                    ),
                ),
            )
    }
}
