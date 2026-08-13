use gpui::{
    AnyElement, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    StyleRefinement, Styled,
};

use crate::v_flex;

/// A header slot for [`NavigationRail`](super::NavigationRail) holding a FAB, logo, or menu button.
#[derive(IntoElement)]
pub struct NavigationRailHeader {
    id: ElementId,
    style: StyleRefinement,
    children: Vec<AnyElement>,
}

impl NavigationRailHeader {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            children: Vec::new(),
        }
    }
}

impl Styled for NavigationRailHeader {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for NavigationRailHeader {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for NavigationRailHeader {
    fn render(self, _window: &mut gpui::Window, _cx: &mut gpui::App) -> impl IntoElement {
        v_flex()
            .id(self.id)
            .items_center()
            .justify_center()
            .w_full()
            .children(self.children)
    }
}
