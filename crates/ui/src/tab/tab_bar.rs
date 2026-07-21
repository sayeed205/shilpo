use std::{cell::RefCell, rc::Rc, time::Duration};

use gpui::{
    Anchor, Animation, AnimationExt as _, AnyElement, App, Background, Bounds, Div, Edges,
    ElementId, InteractiveElement, IntoElement, ParentElement, Pixels, RenderOnce, Role,
    ScrollHandle, SharedString, Stateful, StatefulInteractiveElement as _, StyleRefinement, Styled,
    Window, div, prelude::FluentBuilder as _, px,
};
use rust_i18n::t;
use smallvec::SmallVec;

use super::{Tab, TabVariant};
use crate::animation::Lerp;
use crate::button::{Button, ButtonVariants as _};
use crate::menu::{DropdownMenu as _, PopupMenuItem};
use crate::{
    ActiveTheme, ElementExt, Icon, IconName, Selectable, Sizable, Size, StyledExt, h_flex,
};

/// Tab indicator alignment for [`TabBar`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IndicatorAlignment {
    /// Align indicator to full tab container bounds (M3 Secondary Tab).
    #[default]
    Container,
    /// Align indicator to inner content bounds (M3 Primary Tab).
    Content,
}

struct TabIndicatorBounds {
    container: Bounds<Pixels>,
    tabs: Vec<Bounds<Pixels>>,
    tab_contents: Vec<Bounds<Pixels>>,
}

impl TabIndicatorBounds {
    fn new(num_tabs: usize) -> Self {
        Self {
            container: Bounds::default(),
            tabs: vec![Bounds::default(); num_tabs],
            tab_contents: vec![Bounds::default(); num_tabs],
        }
    }

    fn resize(&mut self, num_tabs: usize) {
        self.tabs.resize(num_tabs, Bounds::default());
        self.tab_contents.resize(num_tabs, Bounds::default());
    }
}

/// A TabBar element that contains multiple [`Tab`] items.
#[derive(IntoElement)]
pub struct TabBar {
    id: ElementId,
    base: Stateful<Div>,
    style: StyleRefinement,
    scroll_handle: Option<ScrollHandle>,
    prefix: Option<AnyElement>,
    suffix: Option<AnyElement>,
    children: SmallVec<[Tab; 2]>,
    last_empty_space: AnyElement,
    selected_index: Option<usize>,
    variant: TabVariant,
    indicator_alignment: IndicatorAlignment,
    size: Size,
    menu: bool,
    on_click: Option<Rc<dyn Fn(&usize, &mut Window, &mut App) + 'static>>,
}

impl TabBar {
    /// Create a new TabBar.
    pub fn new(id: impl Into<ElementId>) -> Self {
        let id = id.into();
        Self {
            id: id.clone(),
            base: div().id(id).px(px(-1.)),
            style: StyleRefinement::default(),
            children: SmallVec::new(),
            scroll_handle: None,
            prefix: None,
            suffix: None,
            variant: TabVariant::default(),
            indicator_alignment: IndicatorAlignment::default(),
            size: Size::default(),
            last_empty_space: div().w_3().into_any_element(),
            selected_index: None,
            on_click: None,
            menu: false,
        }
    }

    /// Set the Tab variant, all children will inherit the variant.
    pub fn with_variant(mut self, variant: TabVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Set indicator alignment (`Content` for M3 Primary Tab, `Container` for M3 Secondary Tab).
    pub fn indicator_alignment(mut self, alignment: IndicatorAlignment) -> Self {
        self.indicator_alignment = alignment;
        self
    }

    /// Configure TabBar as M3 Primary Tab with Underline variant and Content indicator alignment.
    pub fn primary_tab(mut self) -> Self {
        self.variant = TabVariant::Underline;
        self.indicator_alignment = IndicatorAlignment::Content;
        self
    }

    /// Configure TabBar as M3 Secondary Tab with Underline variant and Container indicator alignment.
    pub fn secondary_tab(mut self) -> Self {
        self.variant = TabVariant::Underline;
        self.indicator_alignment = IndicatorAlignment::Container;
        self
    }

    /// Set the Tab variant to Pill, all children will inherit the variant.
    pub fn pill(mut self) -> Self {
        self.variant = TabVariant::Pill;
        self
    }

    /// Set the Tab variant to Outline, all children will inherit the variant.
    pub fn outline(mut self) -> Self {
        self.variant = TabVariant::Outline;
        self
    }

    /// Set the Tab variant to Segmented, all children will inherit the variant.
    pub fn segmented(mut self) -> Self {
        self.variant = TabVariant::Segmented;
        self
    }

    /// Set the Tab variant to Underline, all children will inherit the variant.
    pub fn underline(mut self) -> Self {
        self.variant = TabVariant::Underline;
        self
    }

    /// Set whether to show the menu button when tabs overflow, default is false.
    pub fn menu(mut self, menu: bool) -> Self {
        self.menu = menu;
        self
    }

    /// Track the scroll of the TabBar.
    pub fn track_scroll(mut self, scroll_handle: &ScrollHandle) -> Self {
        self.scroll_handle = Some(scroll_handle.clone());
        self
    }

    /// Set the prefix element of the TabBar
    pub fn prefix(mut self, prefix: impl IntoElement) -> Self {
        self.prefix = Some(prefix.into_any_element());
        self
    }

    /// Set the suffix element of the TabBar
    pub fn suffix(mut self, suffix: impl IntoElement) -> Self {
        self.suffix = Some(suffix.into_any_element());
        self
    }

    /// Add children of the TabBar, all children will inherit the variant.
    pub fn children(mut self, children: impl IntoIterator<Item = impl Into<Tab>>) -> Self {
        self.children.extend(children.into_iter().map(Into::into));
        self
    }

    /// Add child of the TabBar, tab will inherit the variant.
    pub fn child(mut self, child: impl Into<Tab>) -> Self {
        self.children.push(child.into());
        self
    }

    /// Set the selected index of the TabBar.
    pub fn selected_index(mut self, index: usize) -> Self {
        self.selected_index = Some(index);
        self
    }

    /// Set the last empty space element of the TabBar.
    pub fn last_empty_space(mut self, last_empty_space: impl IntoElement) -> Self {
        self.last_empty_space = last_empty_space.into_any_element();
        self
    }

    /// Set the on_click callback of the TabBar, the first parameter is the index of the clicked tab.
    ///
    /// When this is set, the children's on_click will be ignored.
    pub fn on_click<F>(mut self, on_click: F) -> Self
    where
        F: Fn(&usize, &mut Window, &mut App) + 'static,
    {
        self.on_click = Some(Rc::new(on_click));
        self
    }

    /// Render the sliding indicator element for animated tab switching.
    ///
    /// Returns the indicator element together with the current animation
    /// `epoch`, which increments on every tab switch. Tabs key their own
    /// transitions (e.g. text color fade) on this epoch so they restart in sync
    /// with the indicator slide.
    fn render_indicator(
        &self,
        bounds_rc: &Option<Rc<RefCell<TabIndicatorBounds>>>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<(AnyElement, u64)> {
        let has_indicator = matches!(
            self.variant,
            TabVariant::Segmented | TabVariant::Pill | TabVariant::Underline
        );
        let num_tabs = self.children.len();
        let selected_ix = self.selected_index.unwrap_or(usize::MAX);

        if !(has_indicator && num_tabs > 0 && selected_ix < num_tabs) {
            return None;
        }

        let prev_key = format!("{}-tab-prev", self.id);
        let anim_key = format!("{}-tab-anim", self.id);
        let init_key = format!("{}-tab-init", self.id);

        let prev_selected = window.use_keyed_state(prev_key, cx, |_, _| selected_ix);
        // (from_left, from_width, to_left, to_width, epoch)
        let anim_params =
            window.use_keyed_state(anim_key, cx, |_, _| (px(0.), px(0.), px(0.), px(0.), 0u64));
        let initialized = window.use_keyed_state(init_key, cx, |_, _| false);

        // First frame: trigger re-render to capture bounds via on_prepaint
        if !*initialized.read(cx) {
            initialized.update(cx, |v, _| *v = true);
        }

        self.update_anim_params(selected_ix, bounds_rc, &prev_selected, &anim_params, cx);

        let (from_left, from_width, to_left, to_width, epoch) = *anim_params.read(cx);
        if to_width <= px(0.) {
            return None;
        }

        let variant = self.variant;
        let size = self.size;
        let inner_height = variant.inner_height(size);
        let inner_radius = variant.inner_radius(size, cx);

        let indicator_alignment = self.indicator_alignment;
        let indicator = div()
            .absolute()
            .top_0()
            .bottom_0()
            .map(|el| match variant {
                TabVariant::Segmented => el.flex().items_center().child(
                    div()
                        .w_full()
                        .h(inner_height)
                        .bg(cx.theme().surface)
                        .rounded(inner_radius)
                        .shadow_xs(),
                ),
                TabVariant::Pill => el
                    .flex()
                    .items_center()
                    .child(div().size_full().bg(cx.theme().primary).rounded(px(99.))),
                TabVariant::Underline => match indicator_alignment {
                    IndicatorAlignment::Content => el.flex().justify_center().child(
                        div()
                            .absolute()
                            .bottom_0()
                            .h(px(3.))
                            .w_full()
                            .bg(cx.theme().primary)
                            .rounded_full(),
                    ),
                    IndicatorAlignment::Container => el.child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .h(px(2.))
                            .bg(cx.theme().primary),
                    ),
                },
                _ => el,
            })
            .with_animation(
                ElementId::NamedInteger("tab-ind".into(), epoch),
                Animation::new(Duration::from_millis(250))
                    .with_easing(crate::animation::cubic_bezier(0.2, 0.0, 0.0, 1.0)),
                move |el, delta| {
                    let left = Lerp::lerp(&from_left, &to_left, delta);
                    let width = Lerp::lerp(&from_width, &to_width, delta);
                    el.left(left).w(width)
                },
            );

        Some((indicator.into_any_element(), epoch))
    }

    /// Update animation parameters based on current and previous selection.
    fn update_anim_params(
        &self,
        selected_ix: usize,
        bounds_rc: &Option<Rc<RefCell<TabIndicatorBounds>>>,
        prev_selected: &gpui::Entity<usize>,
        anim_params: &gpui::Entity<(Pixels, Pixels, Pixels, Pixels, u64)>,
        cx: &mut App,
    ) {
        let rc = match bounds_rc {
            Some(rc) => rc,
            None => return,
        };

        let prev_ix = *prev_selected.read(cx);
        let bounds = rc.borrow();
        let container = bounds.container;

        if container.size.width == px(0.) {
            if prev_ix != selected_ix {
                prev_selected.update(cx, |v, _| *v = selected_ix);
            }
            return;
        }

        let get_bounds = |ix: usize| -> Option<Bounds<Pixels>> {
            if self.indicator_alignment == IndicatorAlignment::Content {
                if let Some(cb) = bounds.tab_contents.get(ix) {
                    if cb.size.width > px(0.) {
                        return Some(*cb);
                    }
                }
            }
            bounds.tabs.get(ix).copied()
        };

        if prev_ix != selected_ix {
            let from_b = get_bounds(prev_ix);
            let to_b = get_bounds(selected_ix);
            match (from_b, to_b) {
                (Some(from_b), Some(to_b)) => {
                    let from_left = from_b.origin.x - container.origin.x;
                    let from_width = from_b.size.width;
                    let to_left = to_b.origin.x - container.origin.x;
                    let to_width = to_b.size.width;
                    let epoch = anim_params.read(cx).4 + 1;
                    anim_params.update(cx, |v, _| {
                        *v = (from_left, from_width, to_left, to_width, epoch)
                    });
                }
                (None, Some(to_b)) => {
                    let left = to_b.origin.x - container.origin.x;
                    let width = to_b.size.width;
                    anim_params.update(cx, |v, _| *v = (left, width, left, width, v.4));
                }
                _ => {}
            }
            drop(bounds);
            prev_selected.update(cx, |v, _| *v = selected_ix);
            return;
        }

        if let Some(to_b) = get_bounds(selected_ix) {
            let left = to_b.origin.x - container.origin.x;
            let width = to_b.size.width;
            let (_, _, to_left, to_width, epoch) = *anim_params.read(cx);

            if to_width == px(0.) {
                anim_params.update(cx, |v, _| *v = (left, width, left, width, epoch));
                return;
            }

            if left != to_left || width != to_width {
                anim_params.update(cx, |v, _| *v = (left, width, left, width, epoch));
            }
        }
    }
}

impl Styled for TabBar {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Sizable for TabBar {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl RenderOnce for TabBar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let default_gap = match self.size {
            Size::Small | Size::XSmall => px(8.),
            Size::Large => px(16.),
            _ => px(12.),
        };
        let (bg, paddings, gap): (Background, _, _) = match self.variant {
            TabVariant::Tab => {
                let padding = Edges::all(px(0.));
                (cx.theme().surface_container.into(), padding, px(0.))
            }
            TabVariant::Outline => {
                let padding = Edges::all(px(0.));
                (cx.theme().transparent.into(), padding, default_gap)
            }
            TabVariant::Pill => {
                let padding = Edges::all(px(0.));
                (cx.theme().transparent.into(), padding, px(4.))
            }
            TabVariant::Segmented => {
                let padding_x = match self.size {
                    Size::XSmall => px(2.),
                    Size::Small => px(3.),
                    _ => px(4.),
                };
                let padding = Edges {
                    left: padding_x,
                    right: padding_x,
                    ..Default::default()
                };

                (cx.theme().surface_container_high.into(), padding, px(2.))
            }
            TabVariant::Underline => {
                // This gap is same as the tab inner_paddings
                let gap = match self.size {
                    Size::XSmall => px(10.),
                    Size::Small => px(12.),
                    Size::Large => px(20.),
                    _ => px(16.),
                };

                (cx.theme().transparent.into(), Edges::all(px(0.)), gap)
            }
        };

        let has_indicator = matches!(
            self.variant,
            TabVariant::Segmented | TabVariant::Pill | TabVariant::Underline
        );
        let num_tabs = self.children.len();

        // Bounds tracking for tab indicator animation.
        // Uses Rc<RefCell> to avoid triggering re-renders from prepaint writes.
        let bounds_rc = if has_indicator && num_tabs > 0 {
            let rc: Rc<RefCell<TabIndicatorBounds>> = window
                .use_keyed_state(format!("{}-tab-bounds", self.id), cx, |_, _| {
                    Rc::new(RefCell::new(TabIndicatorBounds::new(num_tabs)))
                })
                .read(cx)
                .clone();
            rc.borrow_mut().resize(num_tabs);
            Some(rc)
        } else {
            None
        };

        let indicator = self.render_indicator(&bounds_rc, window, cx);
        let indicator_epoch = indicator.as_ref().map(|(_, epoch)| *epoch).unwrap_or(0);
        let indicator_element = indicator.map(|(el, _)| el);
        let indicator_ready = indicator_element.is_some();

        let has_suffix_or_menu = self.suffix.is_some() || self.menu;
        let mut item_metas: Vec<(Option<SharedString>, Option<Icon>, bool)> = Vec::new();
        let selected_index = self.selected_index;
        let on_click = self.on_click.clone();

        self.base
            .role(Role::TabList)
            .group("tab-bar")
            .relative()
            .flex()
            .items_center()
            .bg(bg)
            .text_color(cx.theme().on_surface)
            .when(
                self.variant == TabVariant::Underline || self.variant == TabVariant::Tab,
                |this| {
                    this.child(
                        div()
                            .id("border-b")
                            .absolute()
                            .left_0()
                            .bottom_0()
                            .size_full()
                            .border_b_1()
                            .border_color(cx.theme().outline_variant),
                    )
                },
            )
            .rounded(self.variant.tab_bar_radius(self.size, cx))
            .paddings(paddings)
            .refine_style(&self.style)
            .when_some(self.prefix, |this, prefix| this.child(prefix))
            .child(
                h_flex().id("tabs").flex_1().overflow_x_hidden().child(
                    h_flex()
                        .id("tabs-inner")
                        .relative()
                        .gap(gap)
                        .overflow_x_scroll()
                        .when_some(self.scroll_handle, |this, scroll_handle| {
                            this.track_scroll(&scroll_handle)
                        })
                        .when_some(bounds_rc.clone(), |this, rc| {
                            this.on_prepaint(move |bounds, _, _| {
                                rc.borrow_mut().container = bounds;
                            })
                        })
                        .when_some(indicator_element, |this, ind| this.child(ind))
                        .children(self.children.into_iter().enumerate().map(|(ix, child)| {
                            item_metas.push((
                                child.label.clone(),
                                child.icon.clone(),
                                child.disabled,
                            ));
                            let tab_bar_prefix = child.tab_bar_prefix.unwrap_or(true);
                            let mut tab = child
                                .ix(ix)
                                .tab_bar_prefix(tab_bar_prefix)
                                .with_variant(self.variant)
                                .with_size(self.size);
                            tab.indicator_active = has_indicator;
                            tab.indicator_ready = indicator_ready;
                            tab.indicator_epoch = indicator_epoch;
                            let tab = tab
                                .when_some(self.selected_index, |this, selected_ix| {
                                    this.selected(selected_ix == ix)
                                })
                                .when_some(self.on_click.clone(), move |this, on_click| {
                                    this.on_click(move |_, window, cx| on_click(&ix, window, cx))
                                });

                            if let Some(ref rc) = bounds_rc {
                                let rc_content = rc.clone();
                                let tab = tab.on_content_prepaint(move |bounds| {
                                    if let Some(slot) =
                                        rc_content.borrow_mut().tab_contents.get_mut(ix)
                                    {
                                        *slot = bounds;
                                    }
                                });
                                let rc = rc.clone();
                                div()
                                    .flex_shrink_0()
                                    .on_prepaint(move |bounds, _, _| {
                                        if let Some(slot) = rc.borrow_mut().tabs.get_mut(ix) {
                                            *slot = bounds;
                                        }
                                    })
                                    .child(tab)
                                    .into_any_element()
                            } else {
                                tab.into_any_element()
                            }
                        }))
                        .when(has_suffix_or_menu, |this| this.child(self.last_empty_space)),
                ),
            )
            .when(self.menu, |this| {
                this.child(
                    Button::new("more")
                        .xsmall()
                        .text()
                        .icon(IconName::KeyboardArrowDown)
                        .dropdown_menu(move |mut this, _, _| {
                            this = this.scrollable(true);
                            for (ix, (label, icon, disabled)) in item_metas.iter().enumerate() {
                                let base = if let Some(label) = label.clone() {
                                    PopupMenuItem::new(label)
                                } else if let Some(icon) = icon.clone() {
                                    PopupMenuItem::element(move |_, _| icon.clone())
                                } else {
                                    PopupMenuItem::new(t!("Dock.Unnamed"))
                                };
                                this = this.item(
                                    base.checked(selected_index == Some(ix))
                                        .disabled(*disabled)
                                        .when_some(on_click.clone(), |this, on_click| {
                                            this.on_click(move |_, window, cx| {
                                                on_click(&ix, window, cx)
                                            })
                                        }),
                                );
                            }

                            this
                        })
                        .anchor(Anchor::TopRight),
                )
            })
            .when_some(self.suffix, |this, suffix| this.child(suffix))
    }
}

#[cfg(test)]
impl TabBar {
    pub(crate) fn get_id(&self) -> &ElementId {
        &self.id
    }

    pub(crate) fn get_variant(&self) -> TabVariant {
        self.variant
    }

    pub(crate) fn get_indicator_alignment(&self) -> IndicatorAlignment {
        self.indicator_alignment
    }

    pub(crate) fn get_size(&self) -> Size {
        self.size
    }

    pub(crate) fn has_menu(&self) -> bool {
        self.menu
    }

    pub(crate) fn children_count(&self) -> usize {
        self.children.len()
    }

    #[allow(dead_code)]
    pub(crate) fn get_child(&self, ix: usize) -> &Tab {
        &self.children[ix]
    }

    pub(crate) fn get_selected_index(&self) -> Option<usize> {
        self.selected_index
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tab_bar_builder() {
        let bar = TabBar::new("test-bar")
            .selected_index(2)
            .menu(true)
            .with_size(Size::Large)
            .child(Tab::new().label("Tab 1"))
            .children(vec![Tab::new().label("Tab 2"), Tab::new().label("Tab 3")]);

        assert_eq!(bar.get_id().to_string(), "test-bar");
        assert_eq!(bar.get_selected_index(), Some(2));
        assert!(bar.has_menu());
        assert_eq!(bar.get_size(), Size::Large);
        assert_eq!(bar.children_count(), 3);
        assert_eq!(bar.get_variant(), TabVariant::default());
        assert_eq!(bar.get_indicator_alignment(), IndicatorAlignment::default());
    }

    #[test]
    fn test_tab_bar_variants() {
        // Underline + Content (primary_tab)
        let bar = TabBar::new("test-bar").primary_tab();
        assert_eq!(bar.get_variant(), TabVariant::Underline);
        assert_eq!(bar.get_indicator_alignment(), IndicatorAlignment::Content);

        // Underline + Container (secondary_tab)
        let bar = TabBar::new("test-bar").secondary_tab();
        assert_eq!(bar.get_variant(), TabVariant::Underline);
        assert_eq!(bar.get_indicator_alignment(), IndicatorAlignment::Container);

        // Pill
        let bar = TabBar::new("test-bar").pill();
        assert_eq!(bar.get_variant(), TabVariant::Pill);

        // Outline
        let bar = TabBar::new("test-bar").outline();
        assert_eq!(bar.get_variant(), TabVariant::Outline);

        // Segmented
        let bar = TabBar::new("test-bar").segmented();
        assert_eq!(bar.get_variant(), TabVariant::Segmented);

        // Underline
        let bar = TabBar::new("test-bar").underline();
        assert_eq!(bar.get_variant(), TabVariant::Underline);
    }

    #[gpui::test]
    fn test_tab_bar_click_handler(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        let clicked = std::sync::Arc::new(std::sync::Mutex::new(false));
        let clicked_idx = std::sync::Arc::new(std::sync::Mutex::new(999));

        let clicked_clone = clicked.clone();
        let clicked_idx_clone = clicked_idx.clone();
        let bar = TabBar::new("test-bar").on_click(move |idx, _, _| {
            *clicked_clone.lock().unwrap() = true;
            *clicked_idx_clone.lock().unwrap() = *idx;
        });

        let on_click = bar.on_click.unwrap();
        cx.update(|window, cx| {
            on_click(&3, window, cx);
            assert!(*clicked.lock().unwrap());
            assert_eq!(*clicked_idx.lock().unwrap(), 3);
        });
    }
}
