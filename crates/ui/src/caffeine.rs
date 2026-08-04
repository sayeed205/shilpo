use crate::{ActiveTheme, Icon, IconName};
use gpui::{
    App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    StatefulInteractiveElement as _, Styled, Window, div, px,
};

pub fn get_caffeine_icon(active: bool) -> IconName {
    if active {
        IconName::CoffeeFill
    } else {
        IconName::Coffee
    }
}

/// Icon-only Caffeine (sleep inhibitor) status widget for Shilpo UI.
#[derive(IntoElement)]
pub struct CaffeineWidget {
    id: ElementId,
    active: bool,
    on_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl CaffeineWidget {
    pub fn new(id: impl Into<ElementId>, active: bool) -> Self {
        Self {
            id: id.into(),
            active,
            on_click: None,
        }
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for CaffeineWidget {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let icon_name = get_caffeine_icon(self.active);

        let base = div()
            .id(self.id)
            .px_2()
            .py_0p5()
            .rounded_full()
            .bg(cx.theme().surface_container_high)
            .text_color(cx.theme().on_surface)
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_center()
            .child(Icon::new(icon_name).size(px(18.)));

        if let Some(on_click) = self.on_click {
            base.on_click(on_click).into_any_element()
        } else {
            base.into_any_element()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_caffeine_icon_selection() {
        assert_eq!(get_caffeine_icon(false), IconName::Coffee);
        assert_eq!(get_caffeine_icon(true), IconName::CoffeeFill);
    }
}
