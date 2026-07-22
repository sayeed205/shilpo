use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    StatefulInteractiveElement, StyleRefinement, Styled, Window, div, px,
};
use shilpo_services::{AudioInfo, NetworkInfo};
use shilpo_ui::{ActiveTheme, Colorize, Icon, IconName, h_flex};

pub type ClickHandler = Box<dyn Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static>;

/// Module 5: Quick System Toggles Capsule (Keyboard, Mic, Bell/DND, WiFi, Settings).
#[derive(IntoElement)]
pub struct StatusTogglesCapsule {
    id: ElementId,
    _audio: AudioInfo,
    _network: NetworkInfo,
    style: StyleRefinement,
    on_click: Option<ClickHandler>,
}

impl StatusTogglesCapsule {
    pub fn new(id: impl Into<ElementId>, audio: AudioInfo, network: NetworkInfo) -> Self {
        Self {
            id: id.into(),
            _audio: audio,
            _network: network,
            style: StyleRefinement::default(),
            on_click: None,
        }
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
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

        let audio_icon = if self._audio.available {
            div().child(Icon::new(IconName::Heart).size(px(14.)))
        } else {
            div()
                .opacity(0.35)
                .child(Icon::new(IconName::Heart).size(px(14.)))
        };

        let network_icon = if self._network.available {
            div().child(Icon::new(IconName::Network).size(px(14.)))
        } else {
            div()
                .opacity(0.35)
                .child(Icon::new(IconName::Network).size(px(14.)))
        };

        let el = h_flex()
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
            .cursor_pointer()
            .hover(|s| s.bg(cx.theme().surface_container_high.opacity(0.98).darken(0.05)))
            .child(Icon::new(IconName::KeyboardArrowDown).size(px(14.)))
            .child(dot())
            .child(audio_icon)
            .child(dot())
            .child(Icon::new(IconName::Bell).size(px(14.)))
            .child(dot())
            .child(network_icon)
            .child(dot())
            .child(Icon::new(IconName::Settings).size(px(14.)));

        if let Some(handler) = self.on_click {
            el.on_click(handler)
        } else {
            el
        }
    }
}
