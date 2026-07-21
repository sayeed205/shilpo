use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, IntoElement, ParentElement, Render,
    Styled, Window, div,
};
use shilpo_ui::{ActiveTheme as _, StyledExt as _, carousel::Carousel, dock::PanelControl, v_flex};

use crate::section;

pub struct CarouselStory {
    focus_handle: FocusHandle,
    active_index: usize,
}

impl CarouselStory {
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            active_index: 0,
        }
    }

    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }
}

impl super::Story for CarouselStory {
    fn title() -> &'static str {
        "Carousel (M3 Expressive)"
    }

    fn description() -> &'static str {
        "Multi-item content carousel with navigation overlays and page indicators."
    }

    fn new_view(window: &mut Window, cx: &mut App) -> Entity<impl Render> {
        Self::view(window, cx)
    }

    fn zoomable() -> Option<PanelControl> {
        None
    }
}

impl Focusable for CarouselStory {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for CarouselStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        v_flex().gap_6().child(
            section("Carousel Demo").max_w_xl().child(
                Carousel::new("car-1", 3)
                    .active_index(self.active_index)
                    .on_index_change(move |new_idx, _, _, cx| {
                        entity.update(cx, |this, cx| {
                            this.active_index = new_idx;
                            cx.notify();
                        });
                    })
                    .item(
                        div()
                            .h_64()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(cx.theme().primary_container)
                            .text_color(cx.theme().on_primary_container)
                            .text_xl()
                            .font_bold()
                            .child("Slide 1 — Expressive Motion"),
                    )
                    .item(
                        div()
                            .h_64()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(cx.theme().secondary_container)
                            .text_color(cx.theme().on_secondary_container)
                            .text_xl()
                            .font_bold()
                            .child("Slide 2 — Material 3 Tokens"),
                    )
                    .item(
                        div()
                            .h_64()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(cx.theme().tertiary_container)
                            .text_color(cx.theme().on_tertiary_container)
                            .text_xl()
                            .font_bold()
                            .child("Slide 3 — GPUI Component Library"),
                    ),
            ),
        )
    }
}
