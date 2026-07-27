use crate::{
    ActiveTheme, Selectable,
    animation::{Transition, cubic_bezier},
    v_flex,
};
use gpui::{
    AnyElement, App, ElementId, InteractiveElement, IntoElement, ParentElement, Pixels, RenderOnce,
    StyleRefinement, Styled, Window, div, px,
};
use std::time::Duration;

mod footer;
mod header;
mod item;

pub use footer::*;
pub use header::*;
pub use item::*;

/// Vertical item arrangement in [`NavigationRail`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NavigationRailArrangement {
    #[default]
    Top,
    Center,
    Bottom,
}

const COLLAPSED_WIDTH: Pixels = px(80.);
const EXPANDED_WIDTH: Pixels = px(240.);

/// Material 3 Expressive Navigation Rail component.
///
/// Provides access to primary destinations in desktop and wide-screen apps.
/// Features smooth spatial active indicator pill gliding matching QML `NavigationRailTabArray.qml`.
#[derive(IntoElement)]
pub struct NavigationRail {
    id: ElementId,
    collapsed: bool,
    show_collapsed_label: bool,
    previous_selected_index: Option<usize>,
    header: Option<AnyElement>,
    footer: Option<AnyElement>,
    items: Vec<NavigationRailItem>,
    arrangement: NavigationRailArrangement,
    style: StyleRefinement,
}

impl NavigationRail {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            collapsed: true,
            show_collapsed_label: false,
            previous_selected_index: None,
            header: None,
            footer: None,
            items: Vec::new(),
            arrangement: NavigationRailArrangement::Top,
            style: StyleRefinement::default(),
        }
    }

    /// Sets whether the rail is in compact collapsed (`80px`) or expanded (`240px`) state.
    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    /// Sets whether to show text labels below item icons when collapsed.
    pub fn show_collapsed_label(mut self, show: bool) -> Self {
        self.show_collapsed_label = show;
        self
    }

    /// Sets the previously selected item index to animate spatial indicator gliding.
    pub fn previous_selected_index(mut self, index: Option<usize>) -> Self {
        self.previous_selected_index = index;
        self
    }

    /// Sets the top header slot (holding a menu toggle button, FAB, or logo).
    pub fn header(mut self, header: impl IntoElement) -> Self {
        self.header = Some(header.into_any_element());
        self
    }

    /// Sets the bottom footer slot.
    pub fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }

    /// Sets the vertical arrangement for items (`Top`, `Center`, or `Bottom`).
    pub fn arrangement(mut self, arrangement: NavigationRailArrangement) -> Self {
        self.arrangement = arrangement;
        self
    }

    /// Adds a navigation item to the rail.
    pub fn item(mut self, item: NavigationRailItem) -> Self {
        self.items.push(item);
        self
    }

    /// Adds multiple navigation items to the rail.
    pub fn items(mut self, items: impl IntoIterator<Item = NavigationRailItem>) -> Self {
        self.items.extend(items);
        self
    }
}

impl Styled for NavigationRail {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for NavigationRail {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let width = if self.collapsed {
            COLLAPSED_WIDTH
        } else {
            EXPANDED_WIDTH
        };

        let is_collapsed = self.collapsed;
        let show_collapsed_label = self.show_collapsed_label;

        // Find active item index to position the single sliding active indicator pill (NavigationRailTabArray.qml behavior)
        let selected_index = self.items.iter().position(|item| item.is_selected());
        let prev_index = self.previous_selected_index.or(selected_index);

        let stride = if is_collapsed {
            if show_collapsed_label {
                px(48.)
            } else {
                px(40.)
            }
        } else {
            px(56.)
        };

        let active_pill = selected_index.map(|idx| {
            let from_idx = prev_index.unwrap_or(idx);
            let from_top = if is_collapsed {
                stride * (from_idx as f32) + px(4.)
            } else {
                stride * (from_idx as f32)
            };

            let to_top = if is_collapsed {
                stride * (idx as f32) + px(4.)
            } else {
                stride * (idx as f32)
            };

            let pill_el = div()
                .w(if is_collapsed { px(56.) } else { px(216.) })
                .h(if is_collapsed { px(32.) } else { px(48.) })
                .rounded_full()
                .bg(cx.theme().secondary_container);

            if from_top != to_top {
                Transition::new(Duration::from_millis(220))
                    .ease(cubic_bezier(0.2, 0.0, 0.0, 1.0))
                    .slide_y(from_top, to_top)
                    .apply(pill_el.absolute().left_0(), ("nav-rail-pill-anim", idx))
                    .into_any_element()
            } else {
                pill_el.absolute().top(to_top).left_0().into_any_element()
            }
        });

        let items: Vec<AnyElement> = self
            .items
            .into_iter()
            .map(|item| {
                item.collapsed(is_collapsed)
                    .show_collapsed_label(show_collapsed_label)
                    .into_any_element()
            })
            .collect();

        let items_container = div()
            .relative()
            .w_full()
            .children(active_pill)
            .child(v_flex().relative().w_full().gap_2().children(items));

        let content_area = match self.arrangement {
            NavigationRailArrangement::Top => items_container,
            NavigationRailArrangement::Center => div().w_full().my_auto().child(items_container),
            NavigationRailArrangement::Bottom => div().w_full().mt_auto().child(items_container),
        };

        v_flex()
            .id(self.id)
            .w(width)
            .h_full()
            .py_4()
            .px_3()
            .gap_4()
            .bg(cx.theme().surface_container)
            .border_r_1()
            .border_color(cx.theme().outline_variant)
            .children(self.header)
            .child(div().flex_1().child(content_area))
            .children(self.footer)
    }
}
