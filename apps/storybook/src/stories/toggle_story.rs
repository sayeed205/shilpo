use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, Focusable, IntoElement,
    ParentElement as _, Render, Styled as _, Window, div, px,
};
use gpui_component::{
    Disableable as _, IconName, Sizable as _,
    button::{Toggle, ToggleGroup, ToggleVariant, ToggleVariants as _},
    v_flex,
};

use crate::section;

pub struct ToggleStory {
    focus_handle: FocusHandle,
    controlled_checked: bool,
}

impl ToggleStory {
    pub fn view(_: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            focus_handle: cx.focus_handle(),
            controlled_checked: false,
        })
    }
}

impl super::Story for ToggleStory {
    fn title() -> &'static str {
        "Toggle"
    }

    fn description() -> &'static str {
        "Material 3 toggle buttons with controlled checks and toolbar groups."
    }

    fn closable() -> bool {
        false
    }

    fn new_view(window: &mut Window, cx: &mut App) -> Entity<impl Render> {
        Self::view(window, cx)
    }
}

impl Focusable for ToggleStory {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

fn labeled(control: impl IntoElement, title: &'static str, note: &'static str) -> impl IntoElement {
    let control: AnyElement = control.into_any_element();
    v_flex()
        .w(px(168.))
        .h(px(132.))
        .items_center()
        .gap_2()
        .child(
            div()
                .w_full()
                .h(px(80.))
                .items_center()
                .justify_center()
                .child(control),
        )
        .child(div().h(px(18.)).child(title))
        .child(div().h(px(18.)).child(note))
}

fn variant_toggle(
    id: &'static str,
    variant: ToggleVariant,
    checked: bool,
    label: &'static str,
) -> Toggle {
    Toggle::new(id)
        .with_variant(variant)
        .label(label)
        .checked(checked)
}

impl Render for ToggleStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let controlled_checked = self.controlled_checked;
        let view = cx.entity();

        let controlled_handler = {
            let view = view.clone();
            move |checked: &bool, _: &mut Window, cx: &mut App| {
                view.update(cx, |view, cx| {
                    view.controlled_checked = *checked;
                    cx.notify();
                });
            }
        };

        v_flex()
            .w_full()
            .gap_6()
            .child(
                section("M3 variants · unchecked and checked")
                    .sub_title("Filled, Elevated, Filled tonal, and Outlined use AndroidX static roles.")
                    .child(labeled(
                        variant_toggle("filled-off", ToggleVariant::Filled, false, "Filled"),
                        "Filled · off",
                        "surface container",
                    ))
                    .child(labeled(
                        variant_toggle("filled-on", ToggleVariant::Filled, true, "Filled"),
                        "Filled · on",
                        "primary container",
                    ))
                    .child(labeled(
                        variant_toggle("elevated-off", ToggleVariant::Elevated, false, "Elevated"),
                        "Elevated · off",
                        "surface + elevation",
                    ))
                    .child(labeled(
                        variant_toggle("elevated-on", ToggleVariant::Elevated, true, "Elevated"),
                        "Elevated · on",
                        "primary container",
                    ))
                    .child(labeled(
                        variant_toggle("tonal-off", ToggleVariant::Tonal, false, "Tonal"),
                        "Filled tonal · off",
                        "secondary container",
                    ))
                    .child(labeled(
                        variant_toggle("tonal-on", ToggleVariant::Tonal, true, "Tonal"),
                        "Filled tonal · on",
                        "secondary role",
                    ))
                    .child(labeled(
                        variant_toggle("outlined-off", ToggleVariant::Outlined, false, "Outlined"),
                        "Outlined · off",
                        "outline + transparent",
                    ))
                    .child(labeled(
                        variant_toggle("outlined-on", ToggleVariant::Outlined, true, "Outlined"),
                        "Outlined · on",
                        "inverse surface",
                    )),
            )
            .child(
                section("M3 metrics and content")
                    .sub_title("Default small metrics: 40px height, 20px icon, 8px icon-to-label gap.")
                    .child(labeled(
                        Toggle::new("metrics-label")
                            .filled()
                            .icon(IconName::Check)
                            .label("Confirmed"),
                        "Icon + label",
                        "40px M3 control",
                    ))
                    .child(labeled(
                        Toggle::new("metrics-icon")
                            .filled_tonal()
                            .icon(IconName::Heart)
                            .checked(true),
                        "Icon only",
                        "20px icon slot",
                    ))
                    .child(labeled(
                        Toggle::new("metrics-large")
                            .outlined()
                            .large()
                            .icon(IconName::Settings2)
                            .label("Settings"),
                        "Large scale",
                        "same content rhythm",
                    )),
            )
            .child(
                section("Shapes")
                    .sub_title("CornerFull default; checked state uses static M3 checked geometry." )
                    .child(labeled(
                        Toggle::new("shape-default")
                            .filled()
                            .label("Default")
                            .checked(false),
                        "Default off",
                        "CornerFull · 20px radius",
                    ))
                    .child(labeled(
                        Toggle::new("shape-default-checked")
                            .filled()
                            .label("Default")
                            .checked(true),
                        "Default on",
                        "static checked shape",
                    ))
                    .child(labeled(
                        Toggle::new("shape-large")
                            .filled_tonal()
                            .large()
                            .label("Large"),
                        "Large static shape",
                        "checked geometry scales with size",
                    )),
            )
            .child(
                section("Controlled and unavailable states")
                    .sub_title("Click controlled example to update on_checked_change; other states are direct examples." )
                    .child(labeled(
                        Toggle::new("controlled")
                            .filled()
                            .icon(IconName::Check)
                            .label("Controlled")
                            .checked(controlled_checked)
                            .on_checked_change(controlled_handler),
                        "Controlled",
                        "updates Storybook state",
                    ))
                    .child(labeled(
                        Toggle::new("disabled")
                            .tonal()
                            .icon(IconName::Bell)
                            .label("Unavailable")
                            .disabled(true),
                        "Disabled",
                        "not allowed",
                    ))
                    .child(labeled(
                        Toggle::new("loading")
                            .filled()
                            .icon(IconName::Loader)
                            .label("Saving")
                            .loading(true),
                        "Loading",
                        "both input and focus blocked",
                    )),
            )
            .child(
                section("ToggleGroup toolbar")
                    .sub_title("Toolbar examples show grouped direct checks and connected segmented presentation." )
                    .child(labeled(
                        ToggleGroup::new("toolbar-m3")
                            .filled()
                            .children([
                                Toggle::new("toolbar-bold")
                                    .icon(IconName::Check)
                                    .checked(true),
                                Toggle::new("toolbar-star")
                                    .icon(IconName::Star),
                                Toggle::new("toolbar-heart")
                                    .icon(IconName::Heart)
                                    .checked(true),
                            ]),
                        "Filled toolbar",
                        "40px grouped controls",
                    ))
                    .child(labeled(
                        ToggleGroup::new("toolbar-segmented")
                            .segmented()
                            .outlined()
                            .children([
                                Toggle::new("toolbar-list").label("List").checked(true),
                                Toggle::new("toolbar-board").label("Board"),
                                Toggle::new("toolbar-calendar").label("Calendar"),
                            ]),
                        "Segmented toolbar",
                        "connected presentation",
                    ))
                    .child(labeled(
                        ToggleGroup::new("toolbar-disabled")
                            .tonal()
                            .disabled(true)
                            .children([
                                Toggle::new("toolbar-disabled-a").label("A").checked(true),
                                Toggle::new("toolbar-disabled-b").label("B"),
                                Toggle::new("toolbar-disabled-c").label("C"),
                            ]),
                        "Disabled toolbar",
                        "bulk disabled state",
                    )),
            )
            .child(
                section("Compatibility-only legacy variants")
                    .sub_title("Ghost and Outline remain available for existing Shilpo callers; they are not M3 parity variants." )
                    .child(labeled(
                        Toggle::new("legacy-ghost")
                            .ghost()
                            .label("Ghost")
                            .checked(true),
                        "Ghost",
                        "legacy compatibility",
                    ))
                    .child(labeled(
                        Toggle::new("legacy-outline")
                            .outline()
                            .label("Outline")
                            .checked(false),
                        "Outline",
                        "legacy compatibility",
                    )),
            )
    }
}
