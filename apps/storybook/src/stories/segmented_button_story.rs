use gpui::{
    App, AppContext as _, Context, Entity, FocusHandle, Focusable, IntoElement,
    ParentElement as _, Render, RenderOnce, Styled, Window,
};
use gpui_component::{
    IconName,
    button::{
        MultiChoiceSegmentedButton, SegmentedButtonItem, SingleChoiceSegmentedButton,
    },
    v_flex,
};

use crate::section;

#[derive(gpui::IntoElement)]
struct SingleSegmentRow {
    button: SingleChoiceSegmentedButton,
}

impl RenderOnce for SingleSegmentRow {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        self.button.render(window, cx)
    }
}

#[derive(gpui::IntoElement)]
struct MultiSegmentRow {
    button: MultiChoiceSegmentedButton,
}

impl RenderOnce for MultiSegmentRow {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        self.button.render(window, cx)
    }
}

pub struct SegmentedButtonStory {
    focus_handle: FocusHandle,
}

impl SegmentedButtonStory {
    pub fn view(_: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            focus_handle: cx.focus_handle(),
        })
    }
}

impl super::Story for SegmentedButtonStory {
    fn title() -> &'static str {
        "Segmented Button"
    }

    fn description() -> &'static str {
        "Connected single-choice and multi-choice Material 3 segments."
    }

    fn new_view(window: &mut Window, cx: &mut App) -> Entity<impl Render> {
        Self::view(window, cx)
    }
}

impl Focusable for SegmentedButtonStory {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SegmentedButtonStory {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .w_full()
            .gap_6()
            .child(
                section("Single choice")
                    .sub_title("One selected segment; equal-width connected row")
                    .child(
                        SingleSegmentRow {
                            button: SingleChoiceSegmentedButton::new("single-choice")
                                .items([
                                SegmentedButtonItem::new("single-day", "Day")
                                    .icon(IconName::Calendar),
                                SegmentedButtonItem::new("single-week", "Week")
                                    .icon(IconName::Calendar)
                                    .selected(true),
                                SegmentedButtonItem::new("single-month", "Month")
                                    .icon(IconName::Calendar),
                                ])
                                .w_full(),
                        },
                    ),
            )
            .child(
                section("Multi choice")
                    .sub_title("Independent checked segments with shared seams")
                    .child(
                        MultiSegmentRow {
                            button: MultiChoiceSegmentedButton::new("multi-choice")
                                .items([
                                SegmentedButtonItem::new("multi-bold", "Bold")
                                    .icon(IconName::Asterisk)
                                    .checked(true),
                                SegmentedButtonItem::new("multi-italic", "Italic")
                                    .icon(IconName::CircleCheck),
                                SegmentedButtonItem::new("multi-underline", "Underline")
                                    .icon(IconName::Minus)
                                    .checked(true),
                                ])
                                .w_full(),
                        },
                    ),
            )
            .child(
                section("Disabled segment")
                    .sub_title("Disabled content retains connected row geometry")
                    .child(
                        SingleSegmentRow {
                            button: SingleChoiceSegmentedButton::new("disabled-segment")
                                .items([
                                SegmentedButtonItem::new("enabled-a", "Available")
                                    .selected(true),
                                SegmentedButtonItem::new("disabled-b", "Unavailable")
                                    .disabled(true),
                                SegmentedButtonItem::new("enabled-c", "More"),
                                ])
                                .w_full(),
                        },
                    ),
            )
    }
}
