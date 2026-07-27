use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, StyleRefinement,
    Styled, Window, div, px,
};
use shilpo_services::NetworkInfo;
use shilpo_ui::{ActiveTheme, Icon, IconName, StyledExt, h_flex};

/// Network status widget for Shilpo status bar.
#[derive(IntoElement)]
pub struct NetworkWidget {
    id: ElementId,
    info: NetworkInfo,
    style: StyleRefinement,
}

impl NetworkWidget {
    pub fn new(id: impl Into<ElementId>, info: NetworkInfo) -> Self {
        Self {
            id: id.into(),
            info,
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for NetworkWidget {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for NetworkWidget {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let label = self.info.ssid.unwrap_or_else(|| "Connected".into());

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
                    .gap_1_5()
                    .items_center()
                    .child(Icon::new(IconName::Lan).size(px(16.)))
                    .child(label),
            )
    }
}
