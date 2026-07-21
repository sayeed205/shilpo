use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, StyleRefinement,
    Styled, Window, div, px,
};
use shilpo_services::BatteryInfo;
use shilpo_ui::{ActiveTheme, Icon, IconName, StyledExt, h_flex};

/// Battery percentage & status widget for Shilpo status bar.
#[derive(IntoElement)]
pub struct BatteryWidget {
    id: ElementId,
    info: BatteryInfo,
    style: StyleRefinement,
}

impl BatteryWidget {
    pub fn new(id: impl Into<ElementId>, info: BatteryInfo) -> Self {
        Self {
            id: id.into(),
            info,
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for BatteryWidget {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for BatteryWidget {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (icon, fg) = if self.info.is_charging {
            (IconName::BatteryCharging, cx.theme().primary)
        } else if self.info.percentage <= 20 {
            (IconName::BatteryWarning, cx.theme().error)
        } else if self.info.percentage >= 80 {
            (IconName::BatteryFull, cx.theme().on_surface)
        } else {
            (IconName::BatteryLow, cx.theme().on_surface)
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
                    .child(format!("{}%", self.info.percentage)),
            )
    }
}
