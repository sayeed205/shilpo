use crate::v_flex;
use gpui::{
    AnyElement, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    StyleRefinement, Styled,
};

/// A footer slot for [`NavigationRail`](super::NavigationRail) holding profile or status elements.
#[derive(IntoElement)]
pub struct NavigationRailFooter {
    id: ElementId,
    style: StyleRefinement,
    children: Vec<AnyElement>,
}

impl NavigationRailFooter {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            children: Vec::new(),
        }
    }
}

impl Styled for NavigationRailFooter {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for NavigationRailFooter {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for NavigationRailFooter {
    fn render(self, _window: &mut gpui::Window, _cx: &mut gpui::App) -> impl IntoElement {
        v_flex()
            .id(self.id)
            .items_center()
            .justify_center()
            .w_full()
            .children(self.children)
    }
}
