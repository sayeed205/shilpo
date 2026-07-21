use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, StyleRefinement,
    Styled, Window, div, px,
};
use shilpo_ui::{ActiveTheme, Icon, IconName, StyledExt, h_flex};

/// Module 1: Window Info Capsule (Sparkle icon + App ID + Window Title).
#[derive(IntoElement)]
pub struct WindowInfoCapsule {
    id: ElementId,
    app_id: String,
    title: String,
    style: StyleRefinement,
}

impl WindowInfoCapsule {
    pub fn new(
        id: impl Into<ElementId>,
        app_id: impl Into<String>,
        title: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            app_id: app_id.into(),
            title: title.into(),
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for WindowInfoCapsule {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for WindowInfoCapsule {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        h_flex()
            .id(self.id)
            .h(px(32.))
            .px_3()
            .items_center()
            .gap_2()
            .rounded_full()
            .bg(cx.theme().surface_container_high.opacity(0.92))
            .border_1()
            .border_color(cx.theme().outline_variant.opacity(0.3))
            .text_color(cx.theme().on_surface)
            .shadow_sm()
            .child(
                div()
                    .w(px(20.))
                    .h(px(20.))
                    .rounded_full()
                    .bg(cx.theme().primary)
                    .text_color(cx.theme().on_primary)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Icon::new(IconName::Star).size(px(12.))),
            )
            .child(
                h_flex()
                    .gap_1_5()
                    .items_center()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().on_surface_variant)
                            .child(format!("{} ·", self.app_id)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().on_surface)
                            .max_w(px(200.))
                            .overflow_hidden()
                            .child(self.title),
                    ),
            )
    }
}
