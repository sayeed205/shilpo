use shilpo_ui::{ActiveTheme, Icon, IconName, StyledExt};
use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, StyleRefinement,
    Styled, Window, div, px,
};

pub fn get_wifi_icon(enabled: bool, connected: bool) -> IconName {
    if !enabled || !connected {
        IconName::AndroidWifi3BarOff
    } else {
        IconName::AndroidWifi3Bar
    }
}

/// Icon-only Wi-Fi status widget for Shilpo UI.
#[derive(IntoElement)]
pub struct NetworkWidget {
    id: ElementId,
    wifi_enabled: bool,
    is_connected: bool,
    style: StyleRefinement,
}

impl NetworkWidget {
    pub fn new(id: impl Into<ElementId>, wifi_enabled: bool, is_connected: bool) -> Self {
        Self {
            id: id.into(),
            wifi_enabled,
            is_connected,
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
        let icon_name = get_wifi_icon(self.wifi_enabled, self.is_connected);

        div()
            .id(self.id)
            .px_2()
            .py_0p5()
            .rounded_full()
            .bg(cx.theme().surface_container_high)
            .text_color(cx.theme().on_surface)
            .flex()
            .items_center()
            .justify_center()
            .child(Icon::new(icon_name).size(px(18.)))
            .refine_style(&self.style)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wifi_icon_selection() {
        assert_eq!(get_wifi_icon(false, false), IconName::AndroidWifi3BarOff);
        assert_eq!(get_wifi_icon(true, true), IconName::AndroidWifi3Bar);
    }
}
