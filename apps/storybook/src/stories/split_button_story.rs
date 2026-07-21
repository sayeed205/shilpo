use gpui::{
    Action, AnyElement, App, AppContext as _, Context, Entity, Focusable, IntoElement,
    ParentElement as _, Render, Styled as _, Window, div, px,
};
use serde::Deserialize;
use shilpo_ui::{
    Disableable as _, IconName, Selectable as _, Sizable as _, Size,
    button::{
        Button, ButtonRounded, ButtonVariants as _, SplitButton, SplitButtonShape,
        SplitButtonShapes,
    },
    v_flex,
};

use crate::section;

#[derive(Clone, Action, PartialEq, Eq, Deserialize)]
#[action(namespace = split_button_story, no_json)]
enum SplitAction {
    Draft,
    Publish,
    Export,
}

pub struct SplitButtonStory {
    focus_handle: gpui::FocusHandle,
}

impl SplitButtonStory {
    pub fn view(_: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            focus_handle: cx.focus_handle(),
        })
    }
}

impl super::Story for SplitButtonStory {
    fn title() -> &'static str {
        "SplitButton"
    }

    fn description() -> &'static str {
        "Material 3 dual-action buttons with independent leading and trailing surfaces."
    }

    fn closable() -> bool {
        false
    }

    fn new_view(window: &mut Window, cx: &mut App) -> Entity<impl Render> {
        Self::view(window, cx)
    }
}

impl Focusable for SplitButtonStory {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

fn labeled(control: impl IntoElement, title: &'static str, note: &'static str) -> impl IntoElement {
    let control: AnyElement = control.into_any_element();
    v_flex()
        .w(px(260.))
        .h(px(190.))
        .items_center()
        .gap_2()
        .child(
            div()
                .w_full()
                .h(px(146.))
                .items_center()
                .justify_center()
                .child(control),
        )
        .child(div().h(px(18.)).child(title))
        .child(div().h(px(18.)).child(note))
}

fn action_button(id: &'static str, label: &'static str) -> Button {
    Button::new(id).label(label).on_click(|_, _, _| {})
}

fn menu_button(id: &'static str) -> Button {
    Button::new(id).dropdown_caret(true).on_click(|_, _, _| {})
}

impl Render for SplitButtonStory {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .w_full()
            .gap_6()
            .child(
                section("Variants")
                    .sub_title("Filled, tonal, outlined, and elevated; each half keeps a 48px minimum width.")
                    .child(labeled(
                        SplitButton::new(
                            "filled",
                            action_button("filled-leading", "Save"),
                            menu_button("filled-trailing"),
                        )
                        .filled()
                            .dropdown_menu(|menu, _, _| {
                            menu.menu("Save", Box::new(SplitAction::Publish))
                                .menu("Save as draft", Box::new(SplitAction::Draft))
                        }),
                        "Filled",
                        "primary container",
                    ))
                    .child(labeled(
                        SplitButton::tonal(
                            "tonal",
                            action_button("tonal-leading", "Share"),
                            menu_button("tonal-trailing"),
                        )
                            .dropdown_menu(|menu, _, _| {
                            menu.menu("Share to Web", Box::new(SplitAction::Publish))
                                .menu("Copy link", Box::new(SplitAction::Draft))
                        }),
                        "Filled tonal",
                        "secondary container",
                    ))
                    .child(labeled(
                        SplitButton::outlined(
                            "outlined",
                            action_button("outlined-leading", "Filter"),
                            menu_button("outlined-trailing"),
                        )
                            .dropdown_menu(|menu, _, _| {
                            menu.menu("Filter by name", Box::new(SplitAction::Draft))
                                .menu("Filter by date", Box::new(SplitAction::Publish))
                        }),
                        "Outlined",
                        "outline container",
                    ))
                    .child(labeled(
                        SplitButton::elevated(
                            "elevated",
                            action_button("elevated-leading", "Export"),
                            menu_button("elevated-trailing"),
                        )
                            .dropdown_menu(|menu, _, _| {
                            menu.menu("Export as PDF", Box::new(SplitAction::Draft))
                                .menu("Export as CSV", Box::new(SplitAction::Publish))
                        }),
                        "Elevated",
                        "surface container + elevation",
                    )),
            )
            .child(
                section("With Icon")
                    .sub_title("Split buttons can include leading icons in their action button.")
                    .child(labeled(
                        SplitButton::new(
                            "icon-filled",
                            Button::new("icon-filled-leading")
                                .icon(IconName::Plus)
                                .label("Create")
                                .on_click(|_, _, _| {}),
                            menu_button("icon-filled-trailing"),
                        )
                        .filled()
                            .dropdown_menu(|menu, _, _| {
                            menu.menu("Create project", Box::new(SplitAction::Publish))
                                .menu("Create file", Box::new(SplitAction::Draft))
                        }),
                        "Filled with Icon",
                        "leading icon + label",
                    ))
                    .child(labeled(
                        SplitButton::outlined(
                            "icon-outlined",
                            Button::new("icon-outlined-leading")
                                .icon(IconName::Settings)
                                .label("Settings")
                                .on_click(|_, _, _| {}),
                            menu_button("icon-outlined-trailing"),
                        )
                            .dropdown_menu(|menu, _, _| {
                            menu.menu("General settings", Box::new(SplitAction::Draft))
                                .menu("Advanced settings", Box::new(SplitAction::Publish))
                        }),
                        "Outlined with Icon",
                        "leading icon + label",
                    )),
            )
            .child(
                section("Size scale")
                    .sub_title("AndroidX static heights: 32, 40, 56, 96, and 136px.")
                    .child(labeled(
                        SplitButton::new(
                            "xsmall",
                            action_button("xsmall-leading", "XS"),
                            menu_button("xsmall-trailing"),
                        )
                        .xsmall(),
                        "XSmall · 32px",
                        "48px min per half",
                    ))
                    .child(labeled(
                        SplitButton::new(
                            "small",
                            action_button("small-leading", "Small"),
                            menu_button("small-trailing"),
                        )
                        .small(),
                        "Small · 40px",
                        "48px min per half",
                    ))
                    .child(labeled(
                        SplitButton::new(
                            "medium",
                            action_button("medium-leading", "Medium"),
                            menu_button("medium-trailing"),
                        )
                        .with_size(Size::Medium),
                        "Medium · 56px",
                        "48px min per half",
                    ))
                    .child(labeled(
                        SplitButton::new(
                            "large",
                            action_button("large-leading", "Large"),
                            menu_button("large-trailing"),
                        )
                        .large(),
                        "Large · 96px",
                        "48px min per half",
                    ))
                    .child(labeled(
                        SplitButton::new(
                            "xlarge",
                            action_button("xlarge-leading", "XLarge"),
                            menu_button("xlarge-trailing"),
                        )
                        .with_size(Size::Size(px(136.))),
                        "XLarge · 136px",
                        "48px min per half",
                    )),
            )
            .child(
                section("Independent actions and spacing")
                    .sub_title("Leading/trailing slots are independent; default separation is 2px, custom spacing replaces it without an extra divider.")
                    .child(labeled(
                        SplitButton::new(
                            "callbacks",
                            action_button("callbacks-leading", "Apply"),
                            menu_button("callbacks-trailing"),
                        ),
                        "Separate callbacks",
                        "leading action · trailing action",
                    ))
                    .child(labeled(
                        SplitButton::new(
                            "spaced",
                            action_button("spaced-leading", "Apply"),
                            menu_button("spaced-trailing"),
                        )
                        .spacing(px(8.)),
                        "Custom 8px spacing",
                        "visible gap · no extra divider",
                    )),
            )
            .child(
                section("Shape and trailing state")
                    .sub_title("Default outer CornerFull; inner static corners can be overridden. Trailing checked is supported through Button.")
                    .child(labeled(
                        SplitButton::new(
                            "shape-default",
                            action_button("shape-default-leading", "Default"),
                            menu_button("shape-default-trailing"),
                        ),
                        "Default geometry",
                        "outer pill · inner token",
                    ))
                    .child(labeled(
                        SplitButton::new(
                            "shape-custom",
                            action_button("shape-custom-leading", "Custom"),
                            menu_button("shape-custom-trailing"),
                        )
                        .rounded(ButtonRounded::Medium)
                        .shapes(SplitButtonShapes {
                            shape: SplitButtonShape::Corner(px(12.)),
                            hovered_shape: SplitButtonShape::Corner(px(8.)),
                            pressed_shape: SplitButtonShape::Corner(px(6.)),
                            checked_shape: SplitButtonShape::Corner(px(6.)),
                        }),
                        "Custom static shape",
                        "outer 12px · inner 6px",
                    ))
                    .child(labeled(
                        SplitButton::tonal(
                            "checked-trailing",
                            action_button("checked-leading", "Sort"),
                            menu_button("checked-trailing").selected(true),
                        ),
                        "Trailing checked",
                        "checked state on trailing half",
                    )),
            )
            .child(
                section("Disabled and loading")
                    .sub_title("Direct static states; loading disables both halves.")
                    .child(labeled(
                        SplitButton::new(
                            "disabled",
                            action_button("disabled-leading", "Unavailable"),
                            menu_button("disabled-trailing"),
                        )
                        .disabled(true),
                        "Disabled",
                        "not-allowed interaction",
                    ))
                    .child(labeled(
                        SplitButton::new(
                            "loading",
                            action_button("loading-leading", "Saving"),
                            menu_button("loading-trailing"),
                        )
                        .loading(true),
                        "Loading",
                        "both halves unavailable",
                    )),
            )
            .child(
                section("Shilpo extension · dropdown")
                    .sub_title("Dropdown menu on trailing half is a Shilpo extension beyond static M3 SplitButton.")
                    .child(
                        SplitButton::new(
                            "dropdown-extension",
                            action_button("dropdown-leading", "Publish"),
                            menu_button("dropdown-trailing"),
                        )
                            .dropdown_menu(|menu, _, _| {
                            menu.menu("Save draft", Box::new(SplitAction::Draft))
                                .menu("Publish now", Box::new(SplitAction::Publish))
                                .menu("Export", Box::new(SplitAction::Export))
                        }),
                    ),
        )
    }
}
