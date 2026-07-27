use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, StyleRefinement,
    Styled, Window, div, px,
};
use shilpo_ui::{ActiveTheme, Icon, IconName, StyledExt, h_flex};

/// Clock & Datetime widget for Shilpo status bar.
#[derive(IntoElement)]
pub struct ClockWidget {
    id: ElementId,
    time_str: String,
    style: StyleRefinement,
}

impl ClockWidget {
    pub fn new(id: impl Into<ElementId>, time_str: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            time_str: time_str.into(),
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for ClockWidget {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for ClockWidget {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .id(self.id)
            .px_3()
            .py_1()
            .rounded_full()
            .bg(cx.theme().surface_container_high)
            .text_color(cx.theme().on_surface)
            .text_sm()
            .font_semibold()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(Icon::new(IconName::CalendarToday).size(px(16.)))
                    .child(self.time_str),
            )
    }
}
