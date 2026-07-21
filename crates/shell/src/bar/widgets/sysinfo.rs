use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, StyleRefinement,
    Styled, Window, div, px,
};
use shilpo_ui::{ActiveTheme, Icon, IconName, StyledExt, h_flex};

/// CPU & RAM system monitor widget for Shilpo status bar.
#[derive(IntoElement)]
pub struct SysInfoWidget {
    id: ElementId,
    cpu_percent: u8,
    ram_percent: u8,
    style: StyleRefinement,
}

impl SysInfoWidget {
    pub fn new(id: impl Into<ElementId>, cpu_percent: u8, ram_percent: u8) -> Self {
        Self {
            id: id.into(),
            cpu_percent,
            ram_percent,
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for SysInfoWidget {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for SysInfoWidget {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .id(self.id)
            .px_3()
            .py_1()
            .rounded_full()
            .bg(cx.theme().surface_container_high)
            .text_color(cx.theme().on_surface)
            .text_xs()
            .font_bold()
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(Icon::new(IconName::Cpu).size(px(14.)))
                            .child(format!("{}%", self.cpu_percent)),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(Icon::new(IconName::MemoryStick).size(px(14.)))
                            .child(format!("{}%", self.ram_percent)),
                    ),
            )
    }
}
