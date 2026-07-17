use gpui::{
    App, AppContext as _, Context, Entity, FocusHandle, Focusable, IntoElement,
    ParentElement as _, Render, Styled as _, Window,
};
use gpui_component::{
    IconName,
    button::{
        MultiChoiceSegmentedButton, SegmentedButtonItem, SingleChoiceSegmentedButton,
    },
    v_flex,
};

use crate::section;

pub struct SegmentedButtonStory {
    focus_handle: FocusHandle,
    single_selected: usize,
    text_selected: usize,
    multi_selected: [bool; 3],
}

impl SegmentedButtonStory {
    pub fn view(_: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            focus_handle: cx.focus_handle(),
            single_selected: 1,
            text_selected: 0,
            multi_selected: [true, false, true],
        })
    }
}

impl super::Story for SegmentedButtonStory {
    fn title() -> &'static str {
        "Segmented Button"
    }

    fn description() -> &'static str {
        "Connected, controlled single-choice and multi-choice Material 3 segments."
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let single_selected = self.single_selected;
        let text_selected = self.text_selected;
        let multi_selected = self.multi_selected;
        let view = cx.entity();
        let single_selection_change = {
            let view = view.clone();
            move |index: usize, _: &gpui::ClickEvent, _: &mut Window, cx: &mut App| {
                view.update(cx, |view, cx| {
                    view.single_selected = index;
                    cx.notify();
                });
            }
        };
        let multi_selection_change = {
            let view = view.clone();
            move |index: usize, _: &gpui::ClickEvent, _: &mut Window, cx: &mut App| {
                view.update(cx, |view, cx| {
                    if let Some(selected) = view.multi_selected.get_mut(index) {
                        *selected = !*selected;
                    }
                    cx.notify();
                });
            }
        };
        let text_selection_change = {
            let view = view.clone();
            move |index: usize, _: &gpui::ClickEvent, _: &mut Window, cx: &mut App| {
                view.update(cx, |view, cx| {
                    view.text_selected = index;
                    cx.notify();
                });
            }
        };

        v_flex()
            .w_full()
            .gap_6()
            .child(
                section("Single choice · controlled")
                    .sub_title("Click one segment; selection moves to that segment.")
                    .child(
                        SingleChoiceSegmentedButton::new("single-choice")
                            .items([
                                SegmentedButtonItem::new("single-day", "Day")
                                    .icon(IconName::Calendar)
                                    .selected(single_selected == 0),
                                SegmentedButtonItem::new("single-week", "Week")
                                    .icon(IconName::ChartPie)
                                    .selected(single_selected == 1),
                                SegmentedButtonItem::new("single-month", "Month")
                                    .icon(IconName::LayoutDashboard)
                                    .selected(single_selected == 2),
                            ])
                            .on_selection_change(single_selection_change)
                            .w_full(),
                    ),
            )
            .child(
                section("Multi choice · controlled")
                    .sub_title("Click segments independently; each checked state is preserved.")
                    .child(
                        MultiChoiceSegmentedButton::new("multi-choice")
                            .items([
                                SegmentedButtonItem::new("multi-ready", "Ready")
                                    .icon(IconName::Check)
                                    .checked(multi_selected[0]),
                                SegmentedButtonItem::new("multi-favorite", "Favorite")
                                    .icon(IconName::Star)
                                    .checked(multi_selected[1]),
                                SegmentedButtonItem::new("multi-alert", "Alert")
                                    .icon(IconName::TriangleAlert)
                                    .checked(multi_selected[2]),
                            ])
                            .on_selection_change(multi_selection_change)
                            .w_full(),
                    ),
            )
            .child(
                section("Text-only segments")
                    .sub_title("Connected equal-width row without icon content.")
                    .child(
                        SingleChoiceSegmentedButton::new("text-only")
                            .items([
                                SegmentedButtonItem::new("text-list", "List")
                                    .selected(text_selected == 0),
                                SegmentedButtonItem::new("text-board", "Board")
                                    .selected(text_selected == 1),
                                SegmentedButtonItem::new("text-calendar", "Calendar")
                                    .selected(text_selected == 2),
                            ])
                            .on_selection_change(text_selection_change)
                            .w_full(),
                    ),
            )
            .child(
                section("Disabled segment")
                    .sub_title("Disabled segment keeps row height, seam, and outer corners.")
                    .child(
                        SingleChoiceSegmentedButton::new("disabled-segment")
                            .items([
                                SegmentedButtonItem::new("available", "Available")
                                    .selected(true),
                                SegmentedButtonItem::new("unavailable", "Unavailable")
                                    .disabled(true),
                                SegmentedButtonItem::new("more", "More"),
                            ])
                            .w_full(),
                    ),
            )
    }
}
