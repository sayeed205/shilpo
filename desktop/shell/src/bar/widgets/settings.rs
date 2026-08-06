use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, StyleRefinement,
    Styled, Window, div, px,
};
use shilpo_ui::{ActiveTheme, Icon, IconName};

/// Settings & Control Center trigger button widget for Shilpo status bar.
#[derive(IntoElement)]
pub struct SettingsWidget {
    id: ElementId,
    style: StyleRefinement,
}

impl SettingsWidget {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for SettingsWidget {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for SettingsWidget {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .id(self.id)
            .w(px(32.))
            .h(px(32.))
            .rounded_full()
            .bg(cx.theme().surface_container_high)
            .text_color(cx.theme().on_surface)
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|s| s.bg(cx.theme().surface_container_highest))
            .child(Icon::new(IconName::Settings).size(px(16.)))
    }
}
