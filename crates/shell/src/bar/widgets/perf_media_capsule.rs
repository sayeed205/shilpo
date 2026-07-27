use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, StyleRefinement,
    Styled, Window, div, prelude::FluentBuilder, px,
};
use shilpo_ui::{ActiveTheme, Icon, IconName, StyledExt, h_flex};

/// Module 2: Performance & Media Capsule (CPU %, RAM %, Swap/Disk %, Track).
#[derive(IntoElement)]
pub struct PerfMediaCapsule {
    id: ElementId,
    cpu_percent: u8,
    ram_percent: u8,
    disk_percent: u8,
    track: String,
    style: StyleRefinement,
}

impl PerfMediaCapsule {
    pub fn new(
        id: impl Into<ElementId>,
        cpu_percent: u8,
        ram_percent: u8,
        disk_percent: u8,
        track: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            cpu_percent,
            ram_percent,
            disk_percent,
            track: track.into(),
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for PerfMediaCapsule {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for PerfMediaCapsule {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        h_flex()
            .id(self.id)
            .h(px(32.))
            .px_3()
            .items_center()
            .gap_3()
            .rounded_full()
            .bg(cx.theme().surface_container_high.opacity(0.92))
            .border_1()
            .border_color(cx.theme().outline_variant.opacity(0.3))
            .text_color(cx.theme().on_surface)
            .shadow_sm()
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(Icon::new(IconName::Memory).size(px(13.)))
                    .child(
                        div()
                            .text_xs()
                            .font_bold()
                            .child(self.cpu_percent.to_string()),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(Icon::new(IconName::Memory).size(px(13.)))
                    .child(
                        div()
                            .text_xs()
                            .font_bold()
                            .child(self.ram_percent.to_string()),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(Icon::new(IconName::HardDrive).size(px(13.)))
                    .child(
                        div()
                            .text_xs()
                            .font_bold()
                            .child(self.disk_percent.to_string()),
                    ),
            )
            .when(!self.track.trim().is_empty(), |this| {
                this.child(
                    h_flex()
                        .px_2()
                        .h(px(22.))
                        .items_center()
                        .gap_1_5()
                        .rounded_full()
                        .bg(cx.theme().secondary_container)
                        .text_color(cx.theme().on_secondary_container)
                        .text_xs()
                        .font_medium()
                        .child(Icon::new(IconName::PlayArrow).size(px(11.)))
                        .child(div().max_w(px(140.)).overflow_hidden().child(self.track)),
                )
            })
    }
}
