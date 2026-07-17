use gpui::{
    Action, Anchor, App, AppContext as _, Context, Entity, Focusable, IntoElement,
    ParentElement as _, Render, Styled as _, Window, prelude::FluentBuilder as _,
};
use serde::Deserialize;
use gpui_component::{
    button::{Button, SplitButton},
    checkbox::Checkbox,
    h_flex, v_flex, Sizable as _, IconName, Disableable, Size,
};
use crate::section;

#[derive(Clone, Action, PartialEq, Eq, Deserialize)]
#[action(namespace = split_button_story, no_json)]
enum SplitAction {
    Item1,
    Item2,
    Item3,
}

pub struct SplitButtonStory {
    focus_handle: gpui::FocusHandle,
    disabled: bool,
    loading: bool,
    compact: bool,
}

impl SplitButtonStory {
    pub fn view(_: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            focus_handle: cx.focus_handle(),
            disabled: false,
            loading: false,
            compact: false,
        })
    }
}

impl super::Story for SplitButtonStory {
    fn title() -> &'static str {
        "SplitButton"
    }

    fn description() -> &'static str {
        "A dual-function button where the left half triggers an action and the right triggers a dropdown menu."
    }

    fn closable() -> bool {
        false
    }

    fn new_view(window: &mut Window, cx: &mut App) -> Entity<impl Render> {
        Self::view(window, cx)
    }
}

impl Focusable for SplitButtonStory {
    fn focus_handle(&self, _: &gpui::App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SplitButtonStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = self.disabled;
        let loading = self.loading;
        let compact = self.compact;

        v_flex()
            .gap_6()
            .child(
                h_flex()
                    .gap_3()
                    .child(
                        Checkbox::new("disabled-split")
                            .label("Disabled")
                            .checked(self.disabled)
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.disabled = !view.disabled;
                                cx.notify();
                            })),
                    )
                    .child(
                        Checkbox::new("loading-split")
                            .label("Loading")
                            .checked(self.loading)
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.loading = !view.loading;
                                cx.notify();
                            })),
                    )
                    .child(
                        Checkbox::new("compact-split")
                            .label("Compact")
                            .checked(self.compact)
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.compact = !view.compact;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                section("Filled SplitButton").child(
                    SplitButton::new(
                        "split-filled",
                        Button::new("lead-filled").label("Save Action"),
                        Button::new("trail-filled").icon(IconName::ChevronDown),
                    )
                    .disabled(disabled)
                    .loading(loading)
                    .when(compact, |this| this.compact())
                    .dropdown_menu_with_anchor(Anchor::BottomRight, |menu, _, _| {
                        menu.menu("Save Draft", Box::new(SplitAction::Item1))
                            .menu("Save and Publish", Box::new(SplitAction::Item2))
                            .menu("Export Options", Box::new(SplitAction::Item3))
                    }),
                ),
            )
            .child(
                section("Tonal SplitButton").child(
                    SplitButton::tonal(
                        "split-tonal",
                        Button::new("lead-tonal").label("Share Option"),
                        Button::new("trail-tonal").icon(IconName::ChevronDown),
                    )
                    .disabled(disabled)
                    .loading(loading)
                    .when(compact, |this| this.compact())
                    .dropdown_menu(move |menu, _, _| {
                        menu.menu("Share Link", Box::new(SplitAction::Item1))
                            .menu("Email Invite", Box::new(SplitAction::Item2))
                    }),
                ),
            )
            .child(
                section("Outlined SplitButton").child(
                    SplitButton::outlined(
                        "split-outlined",
                        Button::new("lead-outlined").label("Outline Action"),
                        Button::new("trail-outlined").icon(IconName::ChevronDown),
                    )
                    .disabled(disabled)
                    .loading(loading)
                    .when(compact, |this| this.compact())
                    .dropdown_menu(move |menu, _, _| {
                        menu.menu("Option 1", Box::new(SplitAction::Item1))
                            .menu("Option 2", Box::new(SplitAction::Item2))
                    }),
                ),
            )
            .child(
                section("Elevated SplitButton").child(
                    SplitButton::elevated(
                        "split-elevated",
                        Button::new("lead-elevated").label("Elevated Action"),
                        Button::new("trail-elevated").icon(IconName::ChevronDown),
                    )
                    .disabled(disabled)
                    .loading(loading)
                    .when(compact, |this| this.compact())
                    .dropdown_menu(move |menu, _, _| {
                        menu.menu("Option A", Box::new(SplitAction::Item1))
                            .menu("Option B", Box::new(SplitAction::Item2))
                    }),
                ),
            )
            .child(
                section("Different Sizes").child(
                    h_flex()
                        .gap_4()
                        .items_start()
                        .child(
                            SplitButton::new(
                                "split-xs",
                                Button::new("lead-xs").label("XSmall"),
                                Button::new("trail-xs").icon(IconName::ChevronDown),
                            )
                            .xsmall()
                            .dropdown_menu(|menu, _, _| {
                                menu.menu("Item A", Box::new(SplitAction::Item1))
                            }),
                        )
                        .child(
                            SplitButton::new(
                                "split-sm",
                                Button::new("lead-sm").label("Small"),
                                Button::new("trail-sm").icon(IconName::ChevronDown),
                            )
                            .small()
                            .dropdown_menu(|menu, _, _| {
                                menu.menu("Item A", Box::new(SplitAction::Item1))
                            }),
                        )
                        .child(
                            SplitButton::new(
                                "split-md",
                                Button::new("lead-md").label("Medium"),
                                Button::new("trail-md").icon(IconName::ChevronDown),
                            )
                            .with_size(Size::Medium)
                            .dropdown_menu(|menu, _, _| {
                                menu.menu("Item A", Box::new(SplitAction::Item1))
                            }),
                        )
                        .child(
                            SplitButton::new(
                                "split-lg",
                                Button::new("lead-lg").label("Large"),
                                Button::new("trail-lg").icon(IconName::ChevronDown),
                            )
                            .large()
                            .dropdown_menu(|menu, _, _| {
                                menu.menu("Item A", Box::new(SplitAction::Item1))
                            }),
                        ),
                ),
            )
    }
}
