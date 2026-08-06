use gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, StatefulInteractiveElement as _, StyleRefinement, Styled, Window, div,
    prelude::FluentBuilder as _, px,
};
use std::rc::Rc;

use crate::{ActiveTheme, Icon, IconName, h_flex};

/// A Material Design 3 Expressive Carousel component.
///
/// Displays a multi-item horizontally scrollable image or content carousel
/// with navigation controls and page indicators.
///
/// # Reference
/// AndroidX `Carousel.kt` — `HorizontalMultiBrowseCarousel`, `UncontainedCarousel`.
#[derive(IntoElement)]
pub struct Carousel {
    id: ElementId,
    item_count: usize,
    active_index: usize,
    on_index_change: Option<Rc<dyn Fn(usize, &ClickEvent, &mut Window, &mut App)>>,
    items: Vec<AnyElement>,
    style: StyleRefinement,
}

impl Carousel {
    /// Creates a new Carousel with the specified ID and total item count.
    pub fn new(id: impl Into<ElementId>, item_count: usize) -> Self {
        Self {
            id: id.into(),
            item_count,
            active_index: 0,
            on_index_change: None,
            items: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// Sets the currently active item index.
    pub fn active_index(mut self, index: usize) -> Self {
        self.active_index = if self.item_count > 0 {
            index % self.item_count
        } else {
            0
        };
        self
    }

    /// Sets index change event listener.
    pub fn on_index_change(
        mut self,
        handler: impl Fn(usize, &ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_index_change = Some(Rc::new(handler));
        self
    }

    /// Appends a child carousel item.
    pub fn item(mut self, item: impl IntoElement) -> Self {
        self.items.push(item.into_any_element());
        self
    }

    /// Appends multiple child carousel items.
    pub fn items(mut self, items: impl IntoIterator<Item = impl IntoElement>) -> Self {
        self.items
            .extend(items.into_iter().map(|item| item.into_any_element()));
        self
    }
}

impl Styled for Carousel {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Carousel {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let active_idx = self.active_index;
        let total = self.item_count.max(self.items.len());
        let on_change = self.on_index_change.clone();

        let prev_idx = if active_idx == 0 {
            total.saturating_sub(1)
        } else {
            active_idx - 1
        };
        let next_idx = if total > 0 {
            (active_idx + 1) % total
        } else {
            0
        };

        // Indicator dots
        let indicators = (0..total).map(|i| {
            let is_active = i == active_idx;
            let (w, bg) = if is_active {
                (px(24.), cx.theme().primary)
            } else {
                (px(8.), cx.theme().on_surface_variant.opacity(0.38))
            };

            let on_change_cb = on_change.clone();

            div()
                .id(("indicator", i))
                .h(px(8.))
                .w(w)
                .rounded_full()
                .bg(bg)
                .cursor_pointer()
                .when_some(on_change_cb, move |this, handler| {
                    this.on_click(move |evt, window, cx| {
                        handler(i, evt, window, cx);
                    })
                })
        });

        let on_prev = on_change.clone();
        let on_next = on_change.clone();

        div()
            .id(self.id)
            .relative()
            .w_full()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .relative()
                    .w_full()
                    .overflow_hidden()
                    .rounded_3xl()
                    .bg(cx.theme().surface_container)
                    .children(
                        self.items.into_iter().enumerate().filter_map(|(i, item)| {
                            if i == active_idx { Some(item) } else { None }
                        }),
                    )
                    .child(
                        // Previous Nav Button overlay
                        div()
                            .id("carousel-prev")
                            .absolute()
                            .left_4()
                            .top(px(100.))
                            .p_3()
                            .rounded_full()
                            .bg(cx.theme().surface_container_highest.opacity(0.9))
                            .text_color(cx.theme().on_surface)
                            .shadow_md()
                            .cursor_pointer()
                            .hover(|s| s.bg(cx.theme().primary).text_color(cx.theme().on_primary))
                            .child(Icon::new(IconName::KeyboardArrowLeft).size(px(22.)))
                            .when_some(on_prev, move |this, handler| {
                                this.on_click(move |evt, window, cx| {
                                    handler(prev_idx, evt, window, cx);
                                })
                            }),
                    )
                    .child(
                        // Next Nav Button overlay
                        div()
                            .id("carousel-next")
                            .absolute()
                            .right_4()
                            .top(px(100.))
                            .p_3()
                            .rounded_full()
                            .bg(cx.theme().surface_container_highest.opacity(0.9))
                            .text_color(cx.theme().on_surface)
                            .shadow_md()
                            .cursor_pointer()
                            .hover(|s| s.bg(cx.theme().primary).text_color(cx.theme().on_primary))
                            .child(Icon::new(IconName::KeyboardArrowRight).size(px(22.)))
                            .when_some(on_next, move |this, handler| {
                                this.on_click(move |evt, window, cx| {
                                    handler(next_idx, evt, window, cx);
                                })
                            }),
                    ),
            )
            .child(
                // Dots container
                h_flex()
                    .justify_center()
                    .items_center()
                    .gap_2()
                    .children(indicators),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_carousel_active_index_wrap() {
        let c = Carousel::new("c-1", 5).active_index(7);
        assert_eq!(c.active_index, 2);
    }
}
