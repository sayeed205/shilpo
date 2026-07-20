use gpui::{
    AnyElement, App, AppContext as _, Axis, Context, Entity, Focusable, IntoElement,
    ParentElement as _, Render, Styled as _, Window, div, px,
};
use shilpo_ui::{
    Disableable as _, Selectable as _, Sizable as _, Size,
    button::{Button, ButtonGroup, ButtonGroupMode, ButtonVariants as _},
    h_flex, v_flex,
};

use crate::section;

pub struct ButtonGroupStory {
    focus_handle: gpui::FocusHandle,
}

impl ButtonGroupStory {
    pub fn view(_: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            focus_handle: cx.focus_handle(),
        })
    }
}

impl super::Story for ButtonGroupStory {
    fn title() -> &'static str {
        "ButtonGroup"
    }

    fn description() -> &'static str {
        "Material 3 button groups with standard spacing, connected seams, and selection states."
    }

    fn closable() -> bool {
        false
    }

    fn new_view(window: &mut Window, cx: &mut App) -> Entity<impl Render> {
        Self::view(window, cx)
    }
}

impl Focusable for ButtonGroupStory {
    fn focus_handle(&self, _: &gpui::App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

fn labeled_group(
    group: impl IntoElement,
    title: &'static str,
    note: &'static str,
) -> impl IntoElement {
    let group: AnyElement = group.into_any_element();
    v_flex()
        .w(px(220.))
        .h(px(174.))
        .items_center()
        .gap_2()
        .child(
            div()
                .w_full()
                .h(px(132.))
                .items_center()
                .justify_center()
                .child(group),
        )
        .child(div().h(px(18.)).child(title))
        .child(div().h(px(18.)).child(note))
        .into_any_element()
}

impl Render for ButtonGroupStory {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .w_full()
            .gap_6()
            .child(
                section("Group modes")
                    .sub_title("Standard uses 12px spacing; Connected uses a 2px seam.")
                    .child(labeled_group(
                        ButtonGroup::new("standard-group")
                            .mode(ButtonGroupMode::Standard)
                            .children([
                                Button::new("standard-one").filled().label("One"),
                                Button::new("standard-two").filled().label("Two"),
                                Button::new("standard-three").filled().label("Three"),
                            ]),
                        "Standard",
                        "12px between independent buttons",
                    ))
                    .child(labeled_group(
                        ButtonGroup::new("connected-group")
                            .mode(ButtonGroupMode::Connected)
                            .outlined()
                            .children([
                                Button::new("connected-one").label("One"),
                                Button::new("connected-two").label("Two"),
                                Button::new("connected-three").label("Three"),
                            ]),
                        "Connected",
                        "2px seam with shared outer geometry",
                    ))
                    .child(labeled_group(
                        ButtonGroup::new("connected-vertical")
                            .mode(ButtonGroupMode::Connected)
                            .layout(Axis::Vertical)
                            .filled_tonal()
                            .children([
                                Button::new("vertical-one").label("Top"),
                                Button::new("vertical-two").label("Middle"),
                                Button::new("vertical-three").label("Bottom"),
                            ]),
                        "Connected vertical",
                        "2px seam; outer corners follow axis",
                    )),
            )
            .child(
                section("Selection")
                    .sub_title("Static single-choice and multi-choice examples.")
                    .child(labeled_group(
                        ButtonGroup::new("single-selection")
                            .mode(ButtonGroupMode::Connected)
                            .filled_tonal()
                            .child(Button::new("single-a").label("Day"))
                            .child(Button::new("single-b").label("Week").selected(true))
                            .child(Button::new("single-c").label("Month")),
                        "Single choice",
                        "One selected segment",
                    ))
                    .child(labeled_group(
                        ButtonGroup::new("multi-selection")
                            .mode(ButtonGroupMode::Connected)
                            .multiple(true)
                            .outlined()
                            .child(Button::new("multi-a").label("Email").selected(true))
                            .child(Button::new("multi-b").label("Push"))
                            .child(Button::new("multi-c").label("SMS").selected(true)),
                        "Multiple choice",
                        "Independent selected segments",
                    )),
            )
            .child(
                section("Disabled bulk children")
                    .sub_title("`.children(...)` applies disabled state to every supplied button.")
                    .child(labeled_group(
                        ButtonGroup::new("disabled-children")
                            .mode(ButtonGroupMode::Connected)
                            .disabled(true)
                            .children([
                                Button::new("disabled-one").label("Available"),
                                Button::new("disabled-two").label("Unavailable"),
                                Button::new("disabled-three").label("More"),
                            ]),
                        "Disabled group",
                        "Bulk disabled; geometry remains connected",
                    )),
            )
            .child(
                section("Size coverage")
                    .sub_title("Static groups retain Button size tokens.")
                    .child(
                        h_flex()
                            .flex_wrap()
                            .items_start()
                            .gap_4()
                            .child(labeled_group(
                                ButtonGroup::new("small-group")
                                    .mode(ButtonGroupMode::Standard)
                                    .small()
                                    .children([
                                        Button::new("small-a").label("Small"),
                                        Button::new("small-b").label("Pair"),
                                    ]),
                                "Small",
                                "12px spacing",
                            ))
                            .child(labeled_group(
                                ButtonGroup::new("medium-group")
                                    .mode(ButtonGroupMode::Connected)
                                    .with_size(Size::Medium)
                                    .children([
                                        Button::new("medium-a").label("Medium"),
                                        Button::new("medium-b").label("Pair"),
                                    ]),
                                "Medium",
                                "2px seam",
                            ))
                            .child(labeled_group(
                                ButtonGroup::new("large-group")
                                    .mode(ButtonGroupMode::Connected)
                                    .large()
                                    .children([
                                        Button::new("large-a").label("Large"),
                                        Button::new("large-b").label("Pair"),
                                    ]),
                                "Large",
                                "2px seam",
                            )),
                    ),
            )
    }
}
