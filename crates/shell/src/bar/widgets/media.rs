use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, StyleRefinement,
    Styled, Window, div, px,
};
use shilpo_ui::{ActiveTheme, Icon, IconName, StyledExt, h_flex};

/// MPRIS Media player preview widget for Shilpo status bar.
#[derive(IntoElement)]
pub struct MediaWidget {
    id: ElementId,
    track: String,
    style: StyleRefinement,
}

impl MediaWidget {
    pub fn new(id: impl Into<ElementId>, track: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            track: track.into(),
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for MediaWidget {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for MediaWidget {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .id(self.id)
            .px_3()
            .py_1()
            .rounded_full()
            .bg(cx.theme().secondary_container)
            .text_color(cx.theme().on_secondary_container)
            .text_sm()
            .font_medium()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(Icon::new(IconName::PlayArrow).size(px(14.)))
                    .child(self.track),
            )
    }
}
