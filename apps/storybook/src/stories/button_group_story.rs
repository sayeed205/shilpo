use gpui::{
    App, AppContext as _, Context, Entity, Focusable, IntoElement, ParentElement as _, Render, Styled as _, Window, Axis,
    prelude::FluentBuilder as _,
};
use gpui_component::{
    button::{Button, ButtonGroup, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex, v_flex, Disableable, Selectable as _, Sizable as _, Size,
};
use crate::section;

pub struct ButtonGroupStory {
    focus_handle: gpui::FocusHandle,
    disabled: bool,
    compact: bool,
    outline: bool,
    multiple: bool,
    vertical: bool,
    selected_indices: Vec<usize>,
}

impl ButtonGroupStory {
    pub fn view(_: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            focus_handle: cx.focus_handle(),
            disabled: false,
            compact: false,
            outline: false,
            multiple: false,
            vertical: false,
            selected_indices: vec![0],
        })
    }
}

impl super::Story for ButtonGroupStory {
    fn title() -> &'static str {
        "ButtonGroup"
    }

    fn description() -> &'static str {
        "A group of connected buttons for selection or alignment."
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

impl Render for ButtonGroupStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = self.disabled;
        let compact = self.compact;
        let outline = self.outline;
        let multiple = self.multiple;
        let vertical = self.vertical;
        let selected_indices = self.selected_indices.clone();

        v_flex()
            .gap_6()
            .child(
                h_flex()
                    .gap_3()
                    .child(
                        Checkbox::new("disabled")
                            .label("Disabled")
                            .checked(self.disabled)
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.disabled = !view.disabled;
                                cx.notify();
                            })),
                    )
                    .child(
                        Checkbox::new("compact")
                            .label("Compact")
                            .checked(self.compact)
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.compact = !view.compact;
                                cx.notify();
                            })),
                    )
                    .child(
                        Checkbox::new("outline")
                            .label("Outline")
                            .checked(self.outline)
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.outline = !view.outline;
                                cx.notify();
                            })),
                    )
                    .child(
                        Checkbox::new("multiple")
                            .label("Multiple")
                            .checked(self.multiple)
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.multiple = !view.multiple;
                                if !view.multiple {
                                    view.selected_indices = vec![0];
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        Checkbox::new("vertical")
                            .label("Vertical")
                            .checked(self.vertical)
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.vertical = !view.vertical;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                section("Button Group Variants").child(
                    v_flex()
                        .gap_4()
                        .child(
                            ButtonGroup::new("group-filled")
                                .filled()
                                .multiple(multiple)
                                .disabled(disabled)
                                .when(compact, |this| this.compact())
                                .when(outline, |this| this.outline())
                                .layout(if vertical { Axis::Vertical } else { Axis::Horizontal })
                                .child(Button::new("b1").label("Left").selected(selected_indices.contains(&0)))
                                .child(Button::new("b2").label("Middle").selected(selected_indices.contains(&1)))
                                .child(Button::new("b3").label("Right").selected(selected_indices.contains(&2)))
                                .on_click(cx.listener(|view, ixs: &Vec<usize>, _, cx| {
                                    view.selected_indices = ixs.clone();
                                    cx.notify();
                                })),
                        )
                        .child(
                            ButtonGroup::new("group-tonal")
                                .filled_tonal()
                                .multiple(multiple)
                                .disabled(disabled)
                                .when(compact, |this| this.compact())
                                .when(outline, |this| this.outline())
                                .layout(if vertical { Axis::Vertical } else { Axis::Horizontal })
                                .child(Button::new("t1").label("Yes").selected(selected_indices.contains(&0)))
                                .child(Button::new("t2").label("No").selected(selected_indices.contains(&1)))
                                .on_click(cx.listener(|view, ixs: &Vec<usize>, _, cx| {
                                    view.selected_indices = ixs.clone();
                                    cx.notify();
                                })),
                        )
                        .child(
                            ButtonGroup::new("group-outlined")
                                .outlined()
                                .multiple(multiple)
                                .disabled(disabled)
                                .when(compact, |this| this.compact())
                                .when(outline, |this| this.outline())
                                .layout(if vertical { Axis::Vertical } else { Axis::Horizontal })
                                .child(Button::new("o1").label("Low").selected(selected_indices.contains(&0)))
                                .child(Button::new("o2").label("Medium").selected(selected_indices.contains(&1)))
                                .child(Button::new("o3").label("High").selected(selected_indices.contains(&2)))
                                .on_click(cx.listener(|view, ixs: &Vec<usize>, _, cx| {
                                    view.selected_indices = ixs.clone();
                                    cx.notify();
                                })),
                        ),
                ),
            )
            .child(
                section("Different Sizes").child(
                    h_flex()
                        .gap_4()
                        .items_start()
                        .child(
                            ButtonGroup::new("group-small")
                                .small()
                                .child(Button::new("s1").label("Small A"))
                                .child(Button::new("s2").label("Small B")),
                        )
                        .child(
                            ButtonGroup::new("group-medium")
                                .with_size(Size::Medium)
                                .child(Button::new("m1").label("Medium A"))
                                .child(Button::new("m2").label("Medium B")),
                        )
                        .child(
                            ButtonGroup::new("group-large")
                                .large()
                                .child(Button::new("l1").label("Large A"))
                                .child(Button::new("l2").label("Large B")),
                        ),
                ),
            )
    }
}
