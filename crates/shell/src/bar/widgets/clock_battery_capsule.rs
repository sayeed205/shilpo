use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, StyleRefinement,
    Styled, Window, div, px,
};
use shilpo_services::BatteryInfo;
use shilpo_ui::{ActiveTheme, Icon, IconName, StyledExt, h_flex};

/// Module 4: Clock & Battery Power Capsule (`23:52 · Sat, 11/04 ⚡93`).
#[derive(IntoElement)]
pub struct ClockBatteryCapsule {
    id: ElementId,
    datetime_str: String,
    battery: BatteryInfo,
    style: StyleRefinement,
}

impl ClockBatteryCapsule {
    pub fn new(
        id: impl Into<ElementId>,
        datetime_str: impl Into<String>,
        battery: BatteryInfo,
    ) -> Self {
        Self {
            id: id.into(),
            datetime_str: datetime_str.into(),
            battery,
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for ClockBatteryCapsule {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for ClockBatteryCapsule {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let datetime_display = if self.datetime_str.trim().is_empty() {
            "--:--".to_string()
        } else {
            self.datetime_str
        };

        let (bat_icon, bat_fg) = if self.battery.is_charging {
            (IconName::BatteryAndroidBolt, cx.theme().primary)
        } else if self.battery.percentage <= 20 {
            (IconName::BatteryAndroidAlert, cx.theme().error)
        } else {
            (IconName::BatteryAndroid6, cx.theme().on_surface)
        };

        let battery_pill = if self.battery.is_present {
            h_flex()
                .px_2()
                .h(px(22.))
                .items_center()
                .gap_1()
                .rounded_full()
                .bg(cx.theme().surface_container_highest)
                .text_color(bat_fg)
                .text_xs()
                .font_bold()
                .child(Icon::new(bat_icon).size(px(13.)))
                .child(format!("{}%", self.battery.percentage))
        } else {
            h_flex()
                .px_2()
                .h(px(22.))
                .items_center()
                .gap_1()
                .rounded_full()
                .bg(cx.theme().surface_container_highest.opacity(0.4))
                .text_color(cx.theme().on_surface_variant.opacity(0.5))
                .text_xs()
                .font_medium()
                .child(Icon::new(IconName::BatteryAndroid6).size(px(13.)))
                .child("AC")
        };

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
            .child(div().text_xs().font_semibold().child(datetime_display))
            .child(battery_pill)
    }
}
