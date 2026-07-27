use crate::{
    ActiveTheme, Disableable, Icon, Selectable, StyledExt, h_flex, icon::IconNamed,
    tooltip::Tooltip, v_flex,
};
use gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement as _, IntoElement, ParentElement,
    RenderOnce, SharedString, StatefulInteractiveElement, StyleRefinement, Styled, Window, div,
    hsla, px,
};
use std::rc::Rc;

/// An individual destination item within a [`NavigationRail`](super::NavigationRail).
#[derive(IntoElement)]
pub struct NavigationRailItem {
    id: ElementId,
    icon: Option<Icon>,
    label: Option<SharedString>,
    badge: Option<AnyElement>,
    selected: bool,
    disabled: bool,
    collapsed: bool,
    show_collapsed_label: bool,
    on_click: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
    style: StyleRefinement,
}

impl NavigationRailItem {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            icon: None,
            label: None,
            badge: None,
            selected: false,
            disabled: false,
            collapsed: true,
            show_collapsed_label: false,
            on_click: None,
            style: StyleRefinement::default(),
        }
    }

    /// Sets the icon for this destination.
    pub fn icon(mut self, icon: impl IconNamed) -> Self {
        self.icon = Some(Icon::new(icon));
        self
    }

    /// Sets the text label for this destination.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Adds a badge element (e.g. notification count).
    pub fn badge(mut self, badge: impl IntoElement) -> Self {
        self.badge = Some(badge.into_any_element());
        self
    }

    /// Sets whether the item is in collapsed (compact vertical) mode.
    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    /// Sets whether to show text label below icon in collapsed mode.
    pub fn show_collapsed_label(mut self, show: bool) -> Self {
        self.show_collapsed_label = show;
        self
    }

    /// Sets the click event handler.
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl Selectable for NavigationRailItem {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl Disableable for NavigationRailItem {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Styled for NavigationRailItem {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for NavigationRailItem {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Matching QML NavigationRailButton.qml:
        // Item background is transparent when selected (since NavigationRail renders the single sliding active pill behind items).
        // On hover (unselected), shows subtle hover highlight.
        let fg = if self.selected {
            cx.theme().on_secondary_container
        } else {
            cx.theme().on_surface_variant
        };

        let hover_bg = if self.selected {
            hsla(0.0, 0.0, 0.0, 0.0)
        } else {
            cx.theme().surface_container_high
        };

        let label_str = self.label.clone();

        if self.collapsed {
            // Compact Vertical Layout (QML NavigationRailButton.qml / M3 Baseline: 56x32px item target)
            let icon_element = self.icon.map(|icon| icon.size(px(24.)).text_color(fg));

            let mut pill = div()
                .w(px(56.))
                .h(px(32.))
                .rounded_full()
                .bg(hsla(0.0, 0.0, 0.0, 0.0))
                .hover(|this| this.bg(hover_bg))
                .flex()
                .items_center()
                .justify_center()
                .child(div().relative().children(icon_element));

            if let Some(badge) = self.badge {
                pill = pill.child(div().absolute().top(px(-4.)).right(px(-6.)).child(badge));
            }

            let label_element = if self.show_collapsed_label {
                self.label
                    .map(|label| div().text_xs().font_medium().text_color(fg).child(label))
            } else {
                None
            };

            let mut el = v_flex()
                .id(self.id)
                .items_center()
                .justify_center()
                .gap_1()
                .py_1()
                .px_1()
                .cursor_pointer()
                .child(pill)
                .children(label_element);

            if let Some(label_str) = label_str {
                el =
                    el.tooltip(move |window, cx| Tooltip::new(label_str.clone()).build(window, cx));
            }

            if let Some(handler) = self.on_click {
                el = el.on_click(move |evt, window, cx| handler(evt, window, cx));
            }

            el.into_any_element()
        } else {
            // Expanded Horizontal Layout (QML NavigationRailButton.qml / M3 Baseline: full width pill, inline icon + label)
            let icon_element = self.icon.map(|icon| icon.size(px(24.)).text_color(fg));

            let label_element = self
                .label
                .map(|label| div().text_sm().font_medium().text_color(fg).child(label));

            let mut pill = h_flex()
                .id(self.id)
                .w_full()
                .h(px(48.))
                .px_4()
                .rounded_full()
                .bg(hsla(0.0, 0.0, 0.0, 0.0))
                .hover(|this| this.bg(hover_bg))
                .items_center()
                .gap_3()
                .cursor_pointer()
                .children(icon_element)
                .children(label_element);

            if let Some(badge) = self.badge {
                pill = pill.child(div().ml_auto().child(badge));
            }

            if let Some(handler) = self.on_click {
                pill = pill.on_click(move |evt, window, cx| handler(evt, window, cx));
            }

            pill.into_any_element()
        }
    }
}
