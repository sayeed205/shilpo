use gpui::{
    App, AppContext as _, Context, Entity, Focusable, IntoElement, ParentElement as _, Render,
    Styled as _, Window, px,
};
use gpui_component::{
    Disableable as _, Selectable as _, Sizable as _, Size,
    button::{Button, ButtonGroup, ButtonRounded, ButtonVariant, ButtonVariants as _},
    label::Label,
    v_flex,
};

use crate::section;

pub struct ButtonStory {
    focus_handle: gpui::FocusHandle,
}

impl ButtonStory {
    pub fn view(_: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            focus_handle: cx.focus_handle(),
        })
    }
}

impl super::Story for ButtonStory {
    fn title() -> &'static str {
        "Button"
    }

    fn description() -> &'static str {
        "Material 3 buttons with static, platform-neutral interaction states."
    }

    fn closable() -> bool {
        false
    }

    fn new_view(window: &mut Window, cx: &mut App) -> Entity<impl Render> {
        Self::view(window, cx)
    }
}

impl Focusable for ButtonStory {
    fn focus_handle(&self, _: &gpui::App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ButtonStory {
    fn render(&mut self, _: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let variant_button = |id: &'static str, label: &'static str, variant: ButtonVariant| {
            Button::new(id)
                .with_variant(variant)
                .with_size(Size::Size(px(40.)))
                .h(px(40.))
                .min_w(px(112.))
                .label(label)
        };

        let size_button = |id: &'static str, label: &'static str, height: f32, width: f32| {
            Button::new(id)
                .filled()
                .with_size(Size::Size(px(height)))
                .h(px(height))
                .min_w(px(width))
                .label(label)
        };

        v_flex()
            .w_full()
            .gap_6()
            .child(
                section("Variants · M3 default Small / 40px")
                    .sub_title(Label::new("Five supported AndroidX M3 variants"))
                    .child(
                        v_flex()
                            .items_center()
                            .gap_2()
                            .child(variant_button("filled", "Continue", ButtonVariant::Filled))
                            .child("Filled"),
                    )
                    .child(
                        v_flex()
                            .items_center()
                            .gap_2()
                            .child(variant_button("elevated", "Continue", ButtonVariant::Elevated))
                            .child("Elevated"),
                    )
                    .child(
                        v_flex()
                            .items_center()
                            .gap_2()
                            .child(variant_button(
                                "tonal",
                                "Continue",
                                ButtonVariant::FilledTonal,
                            ))
                            .child("Filled tonal"),
                    )
                    .child(
                        v_flex()
                            .items_center()
                            .gap_2()
                            .child(variant_button(
                                "outlined",
                                "Continue",
                                ButtonVariant::Outlined,
                            ))
                            .child("Outlined"),
                    )
                    .child(
                        v_flex()
                            .items_center()
                            .gap_2()
                            .child(variant_button("text", "Continue", ButtonVariant::Text))
                            .child("Text"),
                    ),
            )
            .child(
                section("Size scale")
                    .sub_title(Label::new("Static showcase heights; large controls stay in separate cells"))
                    .child(
                        v_flex()
                            .items_center()
                            .gap_2()
                            .child(size_button("size-xs", "Tiny", 32., 96.))
                            .child("XSmall · 32px"),
                    )
                    .child(
                        v_flex()
                            .items_center()
                            .gap_2()
                            .child(size_button("size-sm", "Small", 40., 112.))
                            .child("Small · 40px"),
                    )
                    .child(
                        v_flex()
                            .items_center()
                            .gap_2()
                            .child(size_button("size-md", "Medium", 56., 128.))
                            .child("Medium · 56px"),
                    )
                    .child(
                        v_flex()
                            .items_center()
                            .gap_2()
                            .child(size_button("size-lg", "Large", 96., 160.))
                            .child("Large · 96px"),
                    )
                    .child(
                        v_flex()
                            .items_center()
                            .gap_2()
                            .child(size_button("size-xl", "Extra large", 136., 208.))
                            .child("XLarge · 136px"),
                    ),
            )
            .child(
                section("Shape")
                    .sub_title(Label::new(
                        "CornerFull is the AndroidX default; other examples are static caller overrides.",
                    ))
                    .child(
                        v_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("shape-default")
                                    .filled()
                                    .with_size(Size::Size(px(40.)))
                                    .h(px(40.))
                                    .min_w(px(120.))
                                    .label("Default pill"),
                            )
                            .child("CornerFull · AndroidX default"),
                    )
                    .child(
                        v_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("shape-none")
                                    .filled()
                                    .rounded(ButtonRounded::None)
                                    .with_size(Size::Size(px(40.)))
                                    .h(px(40.))
                                    .min_w(px(120.))
                                    .label("Square"),
                            )
                            .child("Custom override · None"),
                    )
                    .child(
                        v_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("shape-small")
                                    .filled()
                                    .rounded(ButtonRounded::Small)
                                    .with_size(Size::Size(px(40.)))
                                    .h(px(40.))
                                    .min_w(px(120.))
                                    .label("Small radius"),
                            )
                            .child("Custom override · Small static radius"),
                    )
                    .child(
                        v_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("shape-size")
                                    .filled()
                                    .rounded(px(6.))
                                    .with_size(Size::Size(px(40.)))
                                    .h(px(40.))
                                    .min_w(px(120.))
                                    .label("6px radius"),
                            )
                            .child("Custom override · exact 6px; no pressed morph"),
                    ),
            )
            .child(
                section("States")
                    .sub_title(Label::new("Static examples of common interaction states."))
                    .child(
                        Button::new("state-enabled")
                            .filled()
                            .with_size(Size::Size(px(40.)))
                            .h(px(40.))
                            .min_w(px(120.))
                            .label("Enabled"),
                    )
                    .child(
                        Button::new("state-disabled")
                            .filled()
                            .with_size(Size::Size(px(40.)))
                            .h(px(40.))
                            .min_w(px(120.))
                            .label("Disabled")
                            .disabled(true),
                    )
                    .child(
                        Button::new("state-loading")
                            .filled()
                            .with_size(Size::Size(px(40.)))
                            .h(px(40.))
                            .min_w(px(120.))
                            .label("Loading")
                            .loading(true),
                    )
                    .child(
                        Button::new("state-selected")
                            .filled_tonal()
                            .with_size(Size::Size(px(40.)))
                            .h(px(40.))
                            .min_w(px(120.))
                            .label("Selected")
                            .selected(true),
                    ),
            )
            .child(
                section("Content and grouping")
                    .sub_title(Label::new("Label, icon, and grouped controls keep the same M3 interaction surface."))
                    .child(Button::new("content-label").filled().label("Label only"))
                    .child(
                        Button::new("content-icon-label")
                            .filled()
                            .icon(gpui_component::IconName::Check)
                            .label("Confirmed"),
                    )
                    .child(
                        Button::new("content-icon")
                            .filled_tonal()
                            .icon(gpui_component::IconName::Plus)
                            .tooltip("Icon only"),
                    )
                    .child(
                        ButtonGroup::new("content-group")
                            .outlined()
                            .child(Button::new("group-one").label("Day"))
                            .child(Button::new("group-two").label("Week").selected(true))
                            .child(Button::new("group-three").label("Month")),
                    ),
            )
    }
}
