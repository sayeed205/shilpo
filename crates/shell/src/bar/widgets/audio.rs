use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, StyleRefinement,
    Styled, Window, div, px,
};
use shilpo_services::AudioInfo;
use shilpo_ui::{ActiveTheme, Icon, IconName, StyledExt, h_flex};

/// Audio volume widget for Shilpo status bar.
#[derive(IntoElement)]
pub struct AudioWidget {
    id: ElementId,
    info: AudioInfo,
    style: StyleRefinement,
}

impl AudioWidget {
    pub fn new(id: impl Into<ElementId>, info: AudioInfo) -> Self {
        Self {
            id: id.into(),
            info,
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for AudioWidget {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for AudioWidget {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (icon, fg) = if self.info.is_muted {
            (IconName::HeartOff, cx.theme().on_surface_variant)
        } else {
            (IconName::Heart, cx.theme().on_surface)
        };

        div()
            .id(self.id)
            .px_3()
            .py_1()
            .rounded_full()
            .bg(cx.theme().surface_container_high)
            .text_color(fg)
            .text_sm()
            .font_semibold()
            .child(
                h_flex()
                    .gap_1_5()
                    .items_center()
                    .child(Icon::new(icon).size(px(16.)))
                    .child(format!("{}%", self.info.volume)),
            )
    }
}
