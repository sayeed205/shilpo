use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, IntoElement, ParentElement, Render,
    Styled, Window, div,
};
use shilpo_ui::{
    Icon, IconName,
    button::Button,
    card::{Card, CardHeader},
    dock::PanelControl,
    h_flex, v_flex,
};

use crate::section;

pub struct CardStory {
    focus_handle: FocusHandle,
}

impl CardStory {
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }
}

impl super::Story for CardStory {
    fn title() -> &'static str {
        "Card (M3 Expressive)"
    }

    fn description() -> &'static str {
        "Material Design 3 Expressive card containers in Filled, Elevated, and Outlined variants."
    }

    fn new_view(window: &mut Window, cx: &mut App) -> Entity<impl Render> {
        Self::view(window, cx)
    }

    fn zoomable() -> Option<PanelControl> {
        None
    }
}

impl Focusable for CardStory {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for CardStory {
    fn render(&mut self, _: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_6()
            .child(
                section("Card Variants (Filled, Elevated, Outlined)").max_w_2xl().child(
                    h_flex()
                        .gap_4()
                        .child(
                            Card::filled()
                                .w_64()
                                .child(CardHeader::new("Filled Card").description("Subtle background contrast."))
                                .child(
                                    div()
                                        .text_sm()
                                        .child("Filled cards provide subtle separation from background content."),
                                ),
                        )
                        .child(
                            Card::elevated()
                                .w_64()
                                .child(CardHeader::new("Elevated Card").description("Shadow depth elevation."))
                                .child(div().text_sm().child("Elevated cards stand out with drop shadows.")),
                        )
                        .child(
                            Card::outlined()
                                .w_64()
                                .child(CardHeader::new("Outlined Card").description("Clean border stroke."))
                                .child(div().text_sm().child("Outlined cards feature a distinct border line.")),
                        ),
                ),
            )
            .child(
                section("Interactive Card").max_w_md().child(
                    Card::elevated()
                        .id("interactive-card")
                        .on_click(|_, _, _| {})
                        .child(CardHeader::new("Clickable Card").description("Hover and press interaction state."))
                        .child(
                            h_flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    h_flex()
                                        .gap_2()
                                        .child(Icon::new(IconName::Star))
                                        .child(div().text_sm().child("Clickable action item")),
                                )
                                .child(Button::new("card-act").child("Explore")),
                        ),
                ),
            )
    }
}
