use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, IntoElement, ParentElement, Render,
    Styled, Window,
};
use shilpo_ui::{Icon, IconName, chip::Chip, dock::PanelControl, h_flex, v_flex};

use crate::section;

pub struct ChipStory {
    focus_handle: FocusHandle,
    selected_filters: Vec<&'static str>,
}

impl ChipStory {
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            selected_filters: vec!["Rust"],
        }
    }

    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }
}

impl super::Story for ChipStory {
    fn title() -> &'static str {
        "Chip (M3 Expressive)"
    }

    fn description() -> &'static str {
        "Material Design 3 Expressive chips for action triggers, filtering, inputs, and suggestions."
    }

    fn new_view(window: &mut Window, cx: &mut App) -> Entity<impl Render> {
        Self::view(window, cx)
    }

    fn zoomable() -> Option<PanelControl> {
        None
    }
}

impl Focusable for ChipStory {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ChipStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();

        v_flex()
            .gap_6()
            .child(
                section("Assist Chips (Contextual Actions)")
                    .max_w_lg()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Chip::assist("assist-1", "Add to Calendar")
                                    .leading_icon(Icon::new(IconName::CalendarToday)),
                            )
                            .child(
                                Chip::assist("assist-2", "Add Tag")
                                    .leading_icon(Icon::new(IconName::Add)),
                            )
                            .child(
                                Chip::assist("assist-3", "Elevated")
                                    .elevated(true)
                                    .leading_icon(Icon::new(IconName::Star)),
                            ),
                    ),
            )
            .child(
                section("Filter Chips (Toggle Selection)")
                    .max_w_lg()
                    .child({
                        let e1 = entity.clone();
                        let e2 = entity.clone();
                        let e3 = entity.clone();
                        let e4 = entity.clone();

                        h_flex()
                            .gap_2()
                            .child(
                                Chip::filter("filter-1", "Rust")
                                    .selected(self.selected_filters.contains(&"Rust"))
                                    .on_click(move |_, _, cx| {
                                        e1.update(cx, |this, cx| {
                                            if let Some(pos) = this
                                                .selected_filters
                                                .iter()
                                                .position(|&t| t == "Rust")
                                            {
                                                this.selected_filters.remove(pos);
                                            } else {
                                                this.selected_filters.push("Rust");
                                            }
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(
                                Chip::filter("filter-2", "TypeScript")
                                    .selected(self.selected_filters.contains(&"TypeScript"))
                                    .on_click(move |_, _, cx| {
                                        e2.update(cx, |this, cx| {
                                            if let Some(pos) = this
                                                .selected_filters
                                                .iter()
                                                .position(|&t| t == "TypeScript")
                                            {
                                                this.selected_filters.remove(pos);
                                            } else {
                                                this.selected_filters.push("TypeScript");
                                            }
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(
                                Chip::filter("filter-3", "Go")
                                    .selected(self.selected_filters.contains(&"Go"))
                                    .on_click(move |_, _, cx| {
                                        e3.update(cx, |this, cx| {
                                            if let Some(pos) = this
                                                .selected_filters
                                                .iter()
                                                .position(|&t| t == "Go")
                                            {
                                                this.selected_filters.remove(pos);
                                            } else {
                                                this.selected_filters.push("Go");
                                            }
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(
                                Chip::filter("filter-4", "Python")
                                    .selected(self.selected_filters.contains(&"Python"))
                                    .on_click(move |_, _, cx| {
                                        e4.update(cx, |this, cx| {
                                            if let Some(pos) = this
                                                .selected_filters
                                                .iter()
                                                .position(|&t| t == "Python")
                                            {
                                                this.selected_filters.remove(pos);
                                            } else {
                                                this.selected_filters.push("Python");
                                            }
                                            cx.notify();
                                        });
                                    }),
                            )
                    }),
            )
            .child(
                section("Input Chips (Removable Tokens)").max_w_lg().child(
                    h_flex()
                        .gap_2()
                        .child(
                            Chip::input("input-1", "antigravity@google.com")
                                .leading_icon(Icon::new(IconName::Person))
                                .on_dismiss(|_, _, _| {}),
                        )
                        .child(
                            Chip::input("input-2", "shilpo-ui")
                                .leading_icon(Icon::new(IconName::Folder))
                                .on_dismiss(|_, _, _| {}),
                        ),
                ),
            )
            .child(
                section("Suggestion Chips").max_w_lg().child(
                    h_flex()
                        .gap_2()
                        .child(Chip::suggestion("sug-1", "How do I build Shilpo?"))
                        .child(Chip::suggestion("sug-2", "Show M3 Expressive tokens"))
                        .child(Chip::suggestion("sug-3", "Run unit tests")),
                ),
            )
    }
}
