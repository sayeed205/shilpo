use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    StatefulInteractiveElement, StyleRefinement, Styled, Window, div, px,
};
use shilpo_ui::{ActiveTheme, Colorize, Icon, IconName};

pub type ClickHandler = Box<dyn Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static>;

/// Launcher button widget for Shilpo status bar.
#[derive(IntoElement)]
pub struct LauncherWidget {
    id: ElementId,
    style: StyleRefinement,
    on_click: Option<ClickHandler>,
}

impl LauncherWidget {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
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

impl Styled for LauncherWidget {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for LauncherWidget {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let el = div()
            .id(self.id)
            .w(px(32.))
            .h(px(32.))
            .rounded_full()
            .bg(cx.theme().primary)
            .text_color(cx.theme().on_primary)
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|s| s.bg(cx.theme().primary.darken(0.1)))
            .child(Icon::new(IconName::Search).size(px(16.)));

        if let Some(handler) = self.on_click {
            el.on_click(handler)
        } else {
            el
        }
    }
}
