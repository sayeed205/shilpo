use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, StyleRefinement,
    Styled, Window, div, px,
};
use shilpo_services::{AudioInfo, NetworkInfo};
use shilpo_ui::{ActiveTheme, Icon, IconName, h_flex};

/// Module 5: Quick System Toggles Capsule (Keyboard, Mic, Bell/DND, WiFi, Settings).
#[derive(IntoElement)]
pub struct StatusTogglesCapsule {
    id: ElementId,
    _audio: AudioInfo,
    _network: NetworkInfo,
    style: StyleRefinement,
}

impl StatusTogglesCapsule {
    pub fn new(id: impl Into<ElementId>, audio: AudioInfo, network: NetworkInfo) -> Self {
        Self {
            id: id.into(),
            _audio: audio,
            _network: network,
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for StatusTogglesCapsule {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for StatusTogglesCapsule {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let dot = || {
            div()
                .text_xs()
                .text_color(cx.theme().outline.opacity(0.6))
                .child("·")
        };

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
            .child(Icon::new(IconName::KeyboardArrowDown).size(px(14.)))
            .child(dot())
            .child(Icon::new(IconName::Heart).size(px(14.)))
            .child(dot())
            .child(Icon::new(IconName::Bell).size(px(14.)))
            .child(dot())
            .child(Icon::new(IconName::Network).size(px(14.)))
            .child(dot())
            .child(Icon::new(IconName::Settings).size(px(14.)))
    }
}
