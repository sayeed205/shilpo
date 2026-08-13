use gpui::{
    Animation, AnimationExt as _, AnyElement, App, ElementId, InteractiveElement, IntoElement,
    ParentElement, RenderOnce, StyleRefinement, Styled, Window, div,
};

use crate::{ActiveTheme, Selectable, v_flex};

mod footer;
mod header;
mod item;
mod menu_button;
mod motion;

pub use footer::*;
pub use header::*;
pub use item::*;
pub use menu_button::*;
use motion::{RailMotionState, RailTarget, spring_progress};

/// Vertical item arrangement in [`NavigationRail`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NavigationRailArrangement {
    #[default]
    Top,
    Center,
    Bottom,
}

/// Material 3 Expressive Navigation Rail component.
///
/// Provides access to primary destinations in desktop and wide-screen apps.
/// The rail owns its retargetable active-indicator motion, so callers only
/// declare the current selection and layout state.
#[derive(IntoElement)]
pub struct NavigationRail {
    id: ElementId,
    collapsed: bool,
    show_collapsed_label: bool,
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
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let is_collapsed = self.collapsed;
        let show_collapsed_label = self.show_collapsed_label;
        let selected_index = self.items.iter().position(|item| item.is_selected());
        let target = RailTarget {
            selected_index,
            collapsed: is_collapsed,
        };
        let motion_key = format!("navigation-rail-motion:{}", self.id);
        let indicator_animation_name = format!("navigation-rail-indicator-motion:{}", self.id);
        let layout_animation_name = format!("navigation-rail-layout-motion:{}", self.id);
        let motion = window.use_keyed_state(motion_key, cx, |_, _| RailMotionState::new(target));
        let snapshot = motion.read(cx).clone();

        if snapshot.target != target {
            let generation = motion.update(cx, |state, _| state.retarget(target));
            let duration = motion.read(cx).duration;
            let motion = motion.clone();
            cx.spawn(async move |cx| {
                cx.background_executor().timer(duration).await;
                _ = motion.update(cx, |state, cx| {
                    if state.generation == generation {
                        state.active = false;
                        state.current.set(target.geometry());
                        cx.notify();
                    }
                });
            })
            .detach();
        }

        let state = motion.read(cx);
        let from_geometry = state.from;
        let target_geometry = target.geometry();
        let generation = state.generation;
        let duration = state.duration;
        let spring = state.spring;
        let active = state.active;

        let active_pill = selected_index.map(|_| {
            let pill = div()
                .absolute()
                .left_0()
                .top(target_geometry.indicator_top)
                .w(target_geometry.indicator_width)
                .h(target_geometry.indicator_height)
                .rounded_full()
                .bg(cx.theme().secondary_container);

            if active {
                pill.with_animation(
                    ElementId::NamedInteger(indicator_animation_name.into(), generation),
                    Animation::new(duration),
                    move |pill, delta| {
                        let progress = spring_progress(delta, duration, spring);
                        let geometry = from_geometry.spring_lerp(target_geometry, progress);
                        pill.top(geometry.indicator_top)
                            .w(geometry.indicator_width)
                            .h(geometry.indicator_height)
                    },
                )
                .into_any_element()
            } else {
                pill.into_any_element()
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

        let rail_container = v_flex()
            .id(self.id)
            .flex_none()
            .w(target_geometry.rail_width)
            .h_full()
            .py_4()
            .px_3()
            .gap_4()
            .bg(cx.theme().surface)
            .overflow_x_hidden()
            .children(self.header)
            .child(div().flex_1().child(content_area))
            .children(self.footer);

        if active {
            let current_geometry = state.current.clone();
            let active_generation = state.active_generation.clone();
            rail_container
                .with_animation(
                    ElementId::NamedInteger(layout_animation_name.into(), generation),
                    Animation::new(duration),
                    move |rail, delta| {
                        let progress = spring_progress(delta, duration, spring);
                        let geometry = from_geometry.spring_lerp(target_geometry, progress);
                        if active_generation.get() == generation {
                            current_geometry.set(geometry);
                        }
                        rail.w(geometry.rail_width)
                    },
                )
                .into_any_element()
        } else {
            rail_container.into_any_element()
        }
    }
}
