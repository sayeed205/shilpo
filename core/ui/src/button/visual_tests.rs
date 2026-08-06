#![cfg(test)]

use gpui::{
    AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render, Styled,
    TestAppContext, VisualTestContext, Window, div, prelude::FluentBuilder as _, px,
};

use crate::button::button_geometry::{self, ButtonSlotGeometry, CornerShape};
use crate::button::{
    Button, ButtonGroup, ButtonGroupMode, ButtonVariant, ButtonVariants, IconButton,
    IconButtonSize, SplitButton, button_tokens, icon_button_dimensions,
};
use crate::{Disableable, IconName, Sizable, Size};

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
}

struct PaintRoot {
    variant: ButtonVariant,
}

impl Render for PaintRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Button::new("paint-button")
            .label("Paint")
            .with_variant(self.variant)
    }
}

#[gpui::test]
fn renderer_consumes_resolved_paint_endpoints_for_all_variants(cx: &mut TestAppContext) {
    for variant in [
        ButtonVariant::Filled,
        ButtonVariant::Elevated,
        ButtonVariant::FilledTonal,
        ButtonVariant::Outlined,
        ButtonVariant::Text,
    ] {
        cx.update(crate::init);
        let _capture = button_tokens::capture_render_paint();
        let (_, visual) = cx.add_window_view(move |_, _| PaintRoot { variant });
        draw(visual);
        let captured = button_tokens::captured_render_paint().unwrap();
        assert_eq!(captured.variant, variant);
        let expected = |state| cx.update(|app| button_tokens::resolved_paint(variant, state, app));
        assert_eq!(
            captured.base,
            expected(button_tokens::ButtonPaintState::Rest)
        );
        assert_eq!(
            captured.hover,
            expected(button_tokens::ButtonPaintState::Hover)
        );
        assert_eq!(
            captured.focus,
            expected(button_tokens::ButtonPaintState::Focus)
        );
        assert_eq!(
            captured.pressed,
            expected(button_tokens::ButtonPaintState::Pressed)
        );
        assert_eq!(
            captured.disabled,
            expected(button_tokens::ButtonPaintState::Disabled)
        );
    }
}

struct GeometryRoot {
    slot: bool,
}

impl Render for GeometryRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let button = Button::new(if self.slot {
            "slot-button"
        } else {
            "standalone-button"
        })
        .label(if self.slot { "Slot" } else { "Standalone" })
        .test_debug_selector(if self.slot {
            "slot-button"
        } else {
            "standalone-button"
        });
        if self.slot {
            div().child(button.h(px(72.)).px(px(31.)).rounded(px(3.)).slot_geometry(
                ButtonSlotGeometry {
                    height: px(48.),
                    min_width: px(80.),
                    padding_start: px(7.),
                    padding_end: px(9.),
                    padding_top: px(5.),
                    padding_bottom: px(6.),
                    corners: CornerShape::all(button_geometry::full()),
                    border_edges: gpui::Edges::all(true),
                },
            ))
        } else {
            div().child(button.h(px(72.)))
        }
    }
}

#[gpui::test]
fn rendered_button_terminal_slot_geometry_precedes_child_style(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let _capture = button_geometry::capture_render_geometry();
    let (_, visual) = cx.add_window_view(|_, _| GeometryRoot { slot: true });
    draw(visual);
    let bounds = visual.debug_bounds("slot-button").unwrap();
    assert_eq!(bounds.size.height, px(48.));
    assert!(bounds.size.width >= px(80.));
    let captured = button_geometry::captured_render_geometry().unwrap();
    assert_eq!(
        (
            captured.height,
            captured.min_width,
            captured.padding_start,
            captured.padding_end,
            captured.padding_top,
            captured.padding_bottom,
            captured.corners,
            captured.border_edges,
        ),
        (
            px(48.),
            px(80.),
            px(7.),
            px(9.),
            px(5.),
            px(6.),
            gpui::Corners::all(px(24.)),
            gpui::Edges::all(true),
        )
    );
    let probe = button_geometry::renderer_probe(
        px(72.),
        px(31.),
        gpui::Edges {
            left: px(31.),
            right: px(31.),
            top: px(0.),
            bottom: px(0.),
        },
        CornerShape::all(button_geometry::full()),
        gpui::Edges::all(true),
        Some(ButtonSlotGeometry {
            height: px(48.),
            min_width: px(80.),
            padding_start: px(7.),
            padding_end: px(9.),
            padding_top: px(5.),
            padding_bottom: px(6.),
            corners: CornerShape::all(button_geometry::full()),
            border_edges: gpui::Edges::all(true),
        }),
    );
    assert_eq!(
        (
            probe.height,
            probe.min_width,
            probe.padding_start,
            probe.padding_end,
            probe.padding_top,
            probe.padding_bottom,
            probe.corners.top_left
        ),
        (px(48.), px(80.), px(7.), px(9.), px(5.), px(6.), px(24.))
    );
}

#[gpui::test]
fn rendered_button_standalone_style_remains_terminal_without_slot(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let _capture = button_geometry::capture_render_geometry();
    let (_, visual) = cx.add_window_view(|_, _| GeometryRoot { slot: false });
    draw(visual);
    assert_eq!(
        visual
            .debug_bounds("standalone-button")
            .unwrap()
            .size
            .height,
        px(72.)
    );
    let captured = button_geometry::captured_render_geometry().unwrap();
    assert_eq!(captured.height, px(72.));
    assert_eq!(captured.corners, gpui::Corners::all(px(36.)));
}

struct AdapterRoot {
    mode: u8,
}

impl Render for AdapterRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let button = Button::new("adapter-button")
            .label("Adapter")
            .outlined()
            .when(self.mode == 0, |button| button.rounded(px(3.)))
            .when(self.mode == 1, |button| {
                button.corner_radii(gpui::Corners {
                    top_left: px(1.),
                    top_right: px(2.),
                    bottom_right: px(3.),
                    bottom_left: px(4.),
                })
            })
            .when(self.mode == 2, |button| {
                button.border_corners(gpui::Corners {
                    top_left: false,
                    top_right: true,
                    bottom_right: false,
                    bottom_left: true,
                })
            })
            .when(self.mode == 3, |button| {
                button
                    .rounded(px(3.))
                    .corner_radii(gpui::Corners::all(px(4.)))
                    .border_corners(gpui::Corners {
                        top_left: false,
                        top_right: false,
                        bottom_right: false,
                        bottom_left: false,
                    })
                    .slot_geometry(ButtonSlotGeometry {
                        height: px(44.),
                        min_width: px(77.),
                        padding_start: px(5.),
                        padding_end: px(6.),
                        padding_top: px(7.),
                        padding_bottom: px(8.),
                        corners: CornerShape {
                            top_left: button_geometry::fixed(px(11.)),
                            top_right: button_geometry::fixed(px(12.)),
                            bottom_right: button_geometry::fixed(px(13.)),
                            bottom_left: button_geometry::fixed(px(14.)),
                        },
                        border_edges: gpui::Edges {
                            left: false,
                            right: true,
                            top: false,
                            bottom: true,
                        },
                    })
            });
        div().child(button)
    }
}

#[gpui::test]
fn production_capture_preserves_legacy_adapters_and_slot_precedence(cx: &mut TestAppContext) {
    for mode in 0..4 {
        cx.update(crate::init);
        let _capture = button_geometry::capture_render_geometry();
        let (_, visual) = cx.add_window_view(move |_, _| AdapterRoot { mode });
        draw(visual);
        let geometry = button_geometry::captured_render_geometry().unwrap();
        match mode {
            0 => assert_eq!(geometry.corners, gpui::Corners::all(px(3.))),
            1 => assert_eq!(
                geometry.corners,
                gpui::Corners {
                    top_left: px(1.),
                    top_right: px(2.),
                    bottom_right: px(3.),
                    bottom_left: px(4.),
                }
            ),
            2 => assert_eq!(
                geometry.corners,
                gpui::Corners {
                    top_left: px(0.),
                    top_right: px(20.),
                    bottom_right: px(0.),
                    bottom_left: px(20.),
                }
            ),
            3 => {
                assert_eq!(geometry.height, px(44.));
                assert_eq!(geometry.min_width, px(77.));
                assert_eq!(
                    geometry.corners,
                    gpui::Corners {
                        top_left: px(11.),
                        top_right: px(12.),
                        bottom_right: px(13.),
                        bottom_left: px(14.),
                    }
                );
                assert_eq!(
                    geometry.border_edges,
                    gpui::Edges {
                        left: false,
                        right: true,
                        top: false,
                        bottom: true,
                    }
                );
            }
            _ => unreachable!(),
        }
    }
}

struct CountState {
    count: usize,
}

struct ButtonRoot {
    state: Entity<CountState>,
    disabled: bool,
    loading: bool,
}

impl Render for ButtonRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.clone();
        div().size_full().child(
            div().debug_selector(|| "visual-button".to_string()).child(
                Button::new("button")
                    .label("Button")
                    .small()
                    .disabled(self.disabled)
                    .loading(self.loading)
                    .on_click(move |_, _, cx| {
                        state.update(cx, |state, _| state.count += 1);
                    }),
            ),
        )
    }
}

fn button_root(
    cx: &mut TestAppContext,
    disabled: bool,
    loading: bool,
) -> (Entity<CountState>, &mut VisualTestContext) {
    cx.update(crate::init);
    let state = cx.new(|_| CountState { count: 0 });
    let state_for_root = state.clone();
    let (_, visual) = cx.add_window_view(move |_, _| ButtonRoot {
        state: state_for_root,
        disabled,
        loading,
    });
    (state, visual)
}

#[gpui::test]
fn rendered_button_small_geometry_and_disabled_loading_guards(cx: &mut TestAppContext) {
    let (state, visual) = button_root(cx, false, false);
    draw(visual);
    let bounds = visual.debug_bounds("visual-button").unwrap();
    assert_eq!(bounds.size.height, px(40.));
    assert!(bounds.size.width >= px(58.));
    visual.simulate_click(bounds.center(), Default::default());
    assert_eq!(state.read_with(visual, |state, _| state.count), 1);

    for (disabled, loading) in [(true, false), (false, true)] {
        let (state, visual) = button_root(cx, disabled, loading);
        draw(visual);
        let bounds = visual.debug_bounds("visual-button").unwrap();
        visual.simulate_click(bounds.center(), Default::default());
        assert_eq!(state.read_with(visual, |state, _| state.count), 0);
    }
}

struct SizeRoot {
    size: Size,
    text: bool,
}

impl Render for SizeRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().child(
            div()
                .debug_selector(|| "visual-size-button".to_string())
                .child(if self.text {
                    Button::new("size-button")
                        .label("Action")
                        .text()
                        .with_size(self.size)
                        .into_any_element()
                } else {
                    Button::new("size-button")
                        .label("Action")
                        .with_size(self.size)
                        .into_any_element()
                }),
        )
    }
}

#[gpui::test]
fn rendered_button_medium_and_text_height_parity(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (_, visual) = cx.add_window_view(|_, _| SizeRoot {
        size: Size::Medium,
        text: false,
    });
    draw(visual);
    let normal = visual.debug_bounds("visual-size-button").unwrap();
    assert_eq!(normal.size.height, px(56.));

    let (_, visual) = cx.add_window_view(|_, _| SizeRoot {
        size: Size::Small,
        text: true,
    });
    draw(visual);
    let text = visual.debug_bounds("visual-size-button").unwrap();
    assert_eq!(text.size.height, px(40.));
}

struct IconRoot {
    state: Entity<CountState>,
    size: IconButtonSize,
    disabled: bool,
    loading: bool,
}

impl Render for IconRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.clone();
        let dimensions = icon_button_dimensions(self.size);
        div().child(
            div()
                .debug_selector(|| "visual-icon-button".to_string())
                .size(dimensions.container)
                .child(
                    IconButton::new("icon")
                        .icon(IconName::Add)
                        .size(self.size)
                        .disabled(self.disabled)
                        .loading(self.loading)
                        .checkable(true)
                        .checked(false)
                        .on_click(move |_, _, cx| {
                            state.update(cx, |state, _| state.count += 1);
                        }),
                ),
        )
    }
}

#[gpui::test]
fn rendered_icon_button_all_sizes_and_disabled_guards(cx: &mut TestAppContext) {
    let expected = [
        (IconButtonSize::XSmall, 32.),
        (IconButtonSize::Small, 40.),
        (IconButtonSize::Medium, 48.),
        (IconButtonSize::Large, 56.),
        (IconButtonSize::XLarge, 72.),
    ];
    for (size, expected_size) in expected {
        cx.update(crate::init);
        let state = cx.new(|_| CountState { count: 0 });
        let state_for_root = state.clone();
        let (_, visual) = cx.add_window_view(move |_, _| IconRoot {
            state: state_for_root,
            size,
            disabled: false,
            loading: false,
        });
        draw(visual);
        let bounds = visual.debug_bounds("visual-icon-button").unwrap();
        assert_eq!(bounds.size.width, px(expected_size));
        assert_eq!(bounds.size.height, px(expected_size));
        visual.simulate_mouse_move(bounds.center(), None, Default::default());
        visual.simulate_click(bounds.center(), Default::default());
        assert_eq!(state.read_with(visual, |state, _| state.count), 1);
    }
    for (disabled, loading) in [(true, false), (false, true)] {
        cx.update(crate::init);
        let state = cx.new(|_| CountState { count: 0 });
        let state_for_root = state.clone();
        let (_, visual) = cx.add_window_view(move |_, _| IconRoot {
            state: state_for_root,
            size: IconButtonSize::Medium,
            disabled,
            loading,
        });
        draw(visual);
        let bounds = visual.debug_bounds("visual-icon-button").unwrap();
        visual.simulate_click(bounds.center(), Default::default());
        assert_eq!(state.read_with(visual, |state, _| state.count), 0);
    }
}

struct SplitRoot {
    state: Entity<CountState>,
}

impl Render for SplitRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.clone();
        div().child(
            div().debug_selector(|| "visual-split".to_string()).child(
                SplitButton::new(
                    "split",
                    Button::new("leading").label("Lead").on_click({
                        let state = state.clone();
                        move |_, _, cx| state.update(cx, |state, _| state.count += 1)
                    }),
                    Button::new("trailing").icon(IconName::KeyboardArrowDown),
                )
                .with_size(Size::Medium)
                .spacing(px(2.)),
            ),
        )
    }
}

#[gpui::test]
fn rendered_split_button_has_stable_height_and_ordered_region(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let state = cx.new(|_| CountState { count: 0 });
    let state_for_root = state.clone();
    let (_, visual) = cx.add_window_view(move |_, _| SplitRoot {
        state: state_for_root,
    });
    draw(visual);
    let bounds = visual.debug_bounds("visual-split").unwrap();
    assert_eq!(bounds.size.height, px(56.));
    assert!(bounds.size.width >= px(96.));
}

struct GroupRoot {
    state: Entity<CountState>,
    mode: ButtonGroupMode,
    vertical: bool,
}

impl Render for GroupRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.clone();
        div().child(
            div().debug_selector(|| "visual-group".to_string()).child(
                ButtonGroup::new("group")
                    .mode(self.mode)
                    .layout(if self.vertical {
                        gpui::Axis::Vertical
                    } else {
                        gpui::Axis::Horizontal
                    })
                    .children([
                        Button::new("one")
                            .label("One")
                            .test_debug_selector("group-one"),
                        Button::new("two")
                            .label("Two")
                            .test_debug_selector("group-two"),
                    ])
                    .on_click(move |_, _, cx| state.update(cx, |state, _| state.count += 1)),
            ),
        )
    }
}

#[gpui::test]
fn rendered_button_group_modes_and_callback(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let state = cx.new(|_| CountState { count: 0 });
    let state_for_root = state.clone();
    let (_, visual) = cx.add_window_view(move |_, _| GroupRoot {
        state: state_for_root,
        mode: ButtonGroupMode::Standard,
        vertical: false,
    });
    draw(visual);
    let bounds = visual.debug_bounds("visual-group").unwrap();
    assert_eq!(bounds.size.height, px(40.));
    let first = visual.debug_bounds("group-one").unwrap();
    let second = visual.debug_bounds("group-two").unwrap();
    assert_eq!(second.origin.x - first.right(), px(12.));
    visual.simulate_click(bounds.center(), Default::default());
    assert_eq!(state.read_with(visual, |state, _| state.count), 1);
}
