use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, Focusable, IntoElement,
    ParentElement, Render, Styled, Window, div, px,
};
use shilpo_ui::{
    ActiveTheme as _, Disableable as _, IconName,
    button::{IconButton, IconButtonSize, IconButtonVariants as _},
    h_flex, v_flex,
};

use crate::section;

pub struct IconButtonStory {
    focus_handle: FocusHandle,
}

impl IconButtonStory {
    pub fn view(_: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            focus_handle: cx.focus_handle(),
        })
    }
}

impl super::Story for IconButtonStory {
    fn title() -> &'static str {
        "Icon Button"
    }

    fn description() -> &'static str {
        "Material 3 icon-only actions with static state layers and toggle states."
    }

    fn new_view(window: &mut Window, cx: &mut App) -> Entity<impl Render> {
        Self::view(window, cx)
    }
}

impl Focusable for IconButtonStory {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for IconButtonStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sizes = [
            ("XSmall", "32px", IconButtonSize::XSmall),
            ("Small", "40px", IconButtonSize::Small),
            ("Medium", "48px", IconButtonSize::Medium),
            ("Large", "56px", IconButtonSize::Large),
            ("XLarge", "72px", IconButtonSize::XLarge),
        ];

        let on_surface_variant = cx.theme().on_surface_variant;
        let cell = move |control: AnyElement, caption: &'static str, note: &'static str| {
            v_flex()
                .w(px(124.))
                .h(px(118.))
                .items_center()
                .gap_1()
                .child(
                    div()
                        .w_full()
                        .h(px(74.))
                        .items_center()
                        .justify_center()
                        .child(control),
                )
                .child(
                    div()
                        .w_full()
                        .h(px(20.))
                        .items_center()
                        .justify_center()
                        .text_sm()
                        .child(caption),
                )
                .child(
                    div()
                        .w_full()
                        .h(px(18.))
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .text_color(on_surface_variant)
                        .child(note),
                )
                .into_any_element()
        };
        let size_cell = move |control: AnyElement, caption: &'static str, note: &'static str| {
            v_flex()
                .flex_1()
                .min_w(px(0.))
                .h(px(118.))
                .items_center()
                .gap_1()
                .child(
                    div()
                        .w_full()
                        .h(px(74.))
                        .items_center()
                        .justify_center()
                        .child(control),
                )
                .child(
                    div()
                        .w_full()
                        .h(px(20.))
                        .items_center()
                        .justify_center()
                        .text_sm()
                        .child(caption),
                )
                .child(
                    div()
                        .w_full()
                        .h(px(18.))
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .text_color(on_surface_variant)
                        .child(note),
                )
                .into_any_element()
        };

        v_flex()
            .w_full()
            .gap_4()
            .child(
                section("Variants")
                    .sub_title("Standard is transparent; other variants own their container.")
                    .child(cell(
                        IconButton::new("variant-standard")
                            .icon(IconName::Search)
                            .standard()
                            .into_any_element(),
                        "Standard",
                        "transparent",
                    ))
                    .child(cell(
                        IconButton::new("variant-filled")
                            .icon(IconName::Check)
                            .filled()
                            .into_any_element(),
                        "Filled",
                        "primary",
                    ))
                    .child(cell(
                        IconButton::new("variant-tonal")
                            .icon(IconName::Star)
                            .filled_tonal()
                            .into_any_element(),
                        "Filled tonal",
                        "secondary",
                    ))
                    .child(cell(
                        IconButton::new("variant-outlined")
                            .icon(IconName::Settings)
                            .outlined()
                            .into_any_element(),
                        "Outlined",
                        "outline",
                    )),
            )
            .child(
                section("Size scale")
                    .sub_title("32, 40, 48, 56, and 72px containers")
                    .child(h_flex().w_full().items_start().gap_1().children(
                        sizes.into_iter().map(|(label, note, size)| {
                            size_cell(
                                IconButton::new(format!("size-{label}"))
                                    .icon(IconName::Settings)
                                    .size(size)
                                    .filled_tonal()
                                    .into_any_element(),
                                label,
                                note,
                            )
                        }),
                    )),
            )
            .child(
                section("Checked and states")
                    .sub_title("Direct examples; no state simulation controls")
                    .child(cell(
                        IconButton::new("toggle-off")
                            .standard()
                            .icon(IconName::Favorite)
                            .checkable(true)
                            .into_any_element(),
                        "Unchecked",
                        "standard",
                    ))
                    .child(cell(
                        IconButton::new("toggle-on")
                            .standard()
                            .icon(IconName::Favorite)
                            .checkable(true)
                            .checked(true)
                            .into_any_element(),
                        "Checked",
                        "selected",
                    ))
                    .child(cell(
                        IconButton::new("disabled")
                            .filled_tonal()
                            .icon(IconName::Notifications)
                            .disabled(true)
                            .into_any_element(),
                        "Disabled",
                        "not allowed",
                    ))
                    .child(cell(
                        IconButton::new("loading")
                            .filled()
                            .icon(IconName::Notifications)
                            .loading(true)
                            .into_any_element(),
                        "Loading",
                        "busy",
                    )),
            )
            .child(
                section("Shapes")
                    .sub_title("Round is default; square keeps M3 static corner tokens")
                    .child(cell(
                        IconButton::new("round")
                            .icon(IconName::Add)
                            .filled_tonal()
                            .into_any_element(),
                        "Round",
                        "default",
                    ))
                    .child(cell(
                        IconButton::new("square")
                            .icon(IconName::Add)
                            .filled_tonal()
                            .shape(shilpo_ui::button::IconButtonShape::Square)
                            .into_any_element(),
                        "Square",
                        "override",
                    )),
            )
            .child(
                section("Widths")
                    .sub_title("Narrow, Default, Wide, and Full Width options")
                    .child(cell(
                        IconButton::new("width-narrow")
                            .icon(IconName::ArrowDownward)
                            .filled_tonal()
                            .narrow()
                            .into_any_element(),
                        "Narrow",
                        "width 36px",
                    ))
                    .child(cell(
                        IconButton::new("width-default")
                            .icon(IconName::ArrowDownward)
                            .filled_tonal()
                            .into_any_element(),
                        "Default",
                        "width 48px",
                    ))
                    .child(cell(
                        IconButton::new("width-wide")
                            .icon(IconName::ArrowDownward)
                            .filled_tonal()
                            .wide()
                            .into_any_element(),
                        "Wide",
                        "width 64px",
                    ))
                    .child(
                        v_flex()
                            .w(px(200.))
                            .items_center()
                            .gap_1()
                            .child(
                                IconButton::new("width-full")
                                    .icon(IconName::ArrowDownward)
                                    .filled_tonal()
                                    .full_width(true),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .items_center()
                                    .justify_center()
                                    .text_sm()
                                    .child("Full Width"),
                            ),
                    ),
            )
    }
}
