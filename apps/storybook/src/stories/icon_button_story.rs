use gpui::{
    App, AppContext as _, Context, Entity, FocusHandle, Focusable, IntoElement, ParentElement,
    Render, Styled, Window,
};
use gpui_component::{
    Disableable as _, IconName,
    button::{IconButton, IconButtonSize, IconButtonVariant, IconButtonVariants as _},
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
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let variants = [
            ("Standard", IconButtonVariant::Standard),
            ("Filled", IconButtonVariant::Filled),
            ("Filled tonal", IconButtonVariant::FilledTonal),
            ("Outlined", IconButtonVariant::Outlined),
        ];
        let sizes = [
            ("XSmall", IconButtonSize::XSmall),
            ("Small", IconButtonSize::Small),
            ("Medium", IconButtonSize::Medium),
            ("Large", IconButtonSize::Large),
            ("XLarge", IconButtonSize::XLarge),
        ];

        v_flex()
            .w_full()
            .gap_6()
            .child(
                section("Variants")
                    .sub_title("Standard, filled, filled tonal, and outlined")
                    .children(variants.into_iter().map(|(label, variant)| {
                        v_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                IconButton::new(format!("variant-{label}"))
                                    .icon(IconName::Heart)
                                    .icon_variant(variant),
                            )
                            .child(label)
                    })),
            )
            .child(
                section("Size scale")
                    .sub_title("32, 40, 48, 56, and 72px containers")
                    .children(sizes.into_iter().map(|(label, size)| {
                        v_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                IconButton::new(format!("size-{label}"))
                                    .icon(IconName::Settings2)
                                    .size(size),
                            )
                            .child(label)
                    })),
            )
            .child(
                section("Checked and states")
                    .sub_title("Direct examples; no state simulation controls")
                    .child(
                        v_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                IconButton::new("toggle-off")
                                    .standard()
                                    .icon(IconName::Heart)
                                    .checkable(true),
                            )
                            .child("Unchecked"),
                    )
                    .child(
                        v_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                IconButton::new("toggle-on")
                                    .standard()
                                    .icon(IconName::Heart)
                                    .checkable(true)
                                    .checked(true),
                            )
                            .child("Checked"),
                    )
                    .child(
                        v_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                IconButton::new("disabled")
                                    .filled_tonal()
                                    .icon(IconName::Bell)
                                    .disabled(true),
                            )
                            .child("Disabled"),
                    )
                    .child(
                        v_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                IconButton::new("loading")
                                    .filled()
                                    .icon(IconName::Loader)
                                    .loading(true),
                            )
                            .child("Loading"),
                    ),
            )
            .child(
                section("Shapes")
                    .sub_title("Round is default; square keeps M3 static corner tokens")
                    .child(
                        h_flex()
                            .gap_4()
                            .child(IconButton::new("round").icon(IconName::Plus))
                            .child(
                                IconButton::new("square")
                                    .icon(IconName::Plus)
                                    .shape(gpui_component::button::IconButtonShape::Square),
                            ),
                    ),
            )
    }
}
