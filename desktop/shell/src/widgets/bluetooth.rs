use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, StyleRefinement,
    Styled, Window, div, px,
};
use shilpo_ui::{ActiveTheme, Icon, IconName, StyledExt};

pub fn get_bluetooth_icon(powered: bool, connected: bool) -> IconName {
    if !powered {
        IconName::BluetoothDisabled
    } else if connected {
        IconName::BluetoothConnected
    } else {
        IconName::Bluetooth
    }
}

/// Icon-only Bluetooth status widget for Shilpo UI.
#[derive(IntoElement)]
pub struct BluetoothWidget {
    id: ElementId,
    powered: bool,
    connected: bool,
    style: StyleRefinement,
}

impl BluetoothWidget {
    pub fn new(id: impl Into<ElementId>, powered: bool, connected: bool) -> Self {
        Self {
            id: id.into(),
            powered,
            connected,
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for BluetoothWidget {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for BluetoothWidget {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let icon_name = get_bluetooth_icon(self.powered, self.connected);

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
    fn test_bluetooth_icon_selection() {
        assert_eq!(
            get_bluetooth_icon(false, false),
            IconName::BluetoothDisabled
        );
        assert_eq!(get_bluetooth_icon(true, false), IconName::Bluetooth);
        assert_eq!(get_bluetooth_icon(true, true), IconName::BluetoothConnected);
    }
}
