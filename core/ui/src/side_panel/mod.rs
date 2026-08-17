use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, ElementId, InteractiveElement, IntoElement, ParentElement, Pixels, RenderOnce,
    Styled, Window, div, px,
};

use crate::ActiveTheme;
use crate::scroll::ScrollableElement;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidePanelPosition {
    Left,
    #[default]
    Right,
}

/// Flexible, reusable SidePanel layout container primitive supporting edge placement,
/// header/footer slots, and Material Design 3 surface container styling.
#[derive(IntoElement)]
pub struct SidePanel {
    id: ElementId,
    position: SidePanelPosition,
    width: Pixels,
    header: Option<AnyElement>,
    footer: Option<AnyElement>,
    children: Vec<AnyElement>,
}

impl SidePanel {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            position: SidePanelPosition::Right,
            width: px(320.0),
            header: None,
            footer: None,
            children: Vec::new(),
        }
    }

    pub fn position(mut self, position: SidePanelPosition) -> Self {
        self.position = position;
        self
    }

    pub fn width(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    pub fn header(mut self, header: impl IntoElement) -> Self {
        self.header = Some(header.into_any_element());
        self
    }

    pub fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }
}

impl ParentElement for SidePanel {
    fn extend(&mut self, children: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(children);
    }
}

impl RenderOnce for SidePanel {
    fn render(self, _window: &mut Window, cx: &mut gpui::App) -> impl IntoElement {
        let theme = cx.theme();
        let bg_color = theme.surface_container;
        let border_color = theme.outline_variant;

        let panel = div()
            .id(self.id)
            .h_full()
            .w(self.width)
            .bg(bg_color)
            .border_color(border_color)
            .flex()
            .flex_col()
            .p_4();

        let panel = match self.position {
            SidePanelPosition::Left => panel.border_r_1(),
            SidePanelPosition::Right => panel.border_l_1(),
        };

        panel
            .when_some(self.header, |this, header| {
                this.child(div().mb_4().child(header))
            })
            .child(
                div()
                    .flex_1()
                    .overflow_y_scrollbar()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .children(self.children),
            )
            .when_some(self.footer, |this, footer| {
                this.child(div().mt_4().child(footer))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_side_panel_primitive_configuration() {
        let panel = SidePanel::new("test_panel")
            .position(SidePanelPosition::Left)
            .width(px(280.0));

        assert_eq!(panel.position, SidePanelPosition::Left);
        assert_eq!(panel.width, px(280.0));
    }
}
