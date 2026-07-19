use std::{cell::Cell, rc::Rc};

use gpui::{
    AnyElement, App, Corners, Edges, ElementId, Hsla, InteractiveElement, IntoElement,
    ParentElement, RenderOnce, Role, SharedString, StatefulInteractiveElement, StyleRefinement,
    Styled, Window, div, prelude::FluentBuilder as _,
};
use smallvec::{SmallVec, smallvec};

use crate::{
    ActiveTheme, Disableable, Icon, Sizable, Size, StyledExt, h_flex, tooltip::ComponentTooltip,
};

use super::{button_dimension_tokens, button_shared_tokens, shared};

#[derive(Default, Copy, Debug, Clone, PartialEq, Eq, Hash)]
pub enum ToggleVariant {
    #[default]
    Filled,
    Elevated,
    Tonal,
    Outlined,
    Ghost,
    Outline,
}

pub trait ToggleVariants: Sized {
    /// Set the variant of the toggle.
    fn with_variant(self, variant: ToggleVariant) -> Self;
    /// Set the variant to ghost.
    fn ghost(self) -> Self {
        self.with_variant(ToggleVariant::Ghost)
    }
    /// Set the variant to outline.
    fn outline(self) -> Self {
        self.with_variant(ToggleVariant::Outline)
    }
    /// Set the variant to filled.
    fn filled(self) -> Self {
        self.with_variant(ToggleVariant::Filled)
    }
    /// Set the variant to elevated.
    fn elevated(self) -> Self {
        self.with_variant(ToggleVariant::Elevated)
    }
    /// Set the variant to tonal.
    fn tonal(self) -> Self {
        self.with_variant(ToggleVariant::Tonal)
    }
    fn filled_tonal(self) -> Self {
        self.with_variant(ToggleVariant::Tonal)
    }
    /// Set the variant to outlined.
    fn outlined(self) -> Self {
        self.with_variant(ToggleVariant::Outlined)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ButtonShape {
    CornerFull,
    Corner(gpui::Pixels),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToggleButtonShapes {
    pub shape: ButtonShape,
    pub pressed_shape: ButtonShape,
    pub checked_shape: ButtonShape,
}

impl ToggleButtonShapes {
    pub fn new(shape: ButtonShape, pressed_shape: ButtonShape, checked_shape: ButtonShape) -> Self {
        Self {
            shape,
            pressed_shape,
            checked_shape,
        }
    }
}

pub fn toggle_button_shapes(size: Size) -> ToggleButtonShapes {
    let checked_radius = match size {
        Size::XSmall | Size::Small => gpui::px(8.),
        Size::Medium => gpui::px(12.),
        Size::Large => gpui::px(16.),
        Size::Size(value) if value <= gpui::px(40.) => gpui::px(8.),
        Size::Size(value) if value <= gpui::px(56.) => gpui::px(12.),
        Size::Size(_) => gpui::px(16.),
    };
    ToggleButtonShapes {
        shape: ButtonShape::CornerFull,
        pressed_shape: ButtonShape::Corner(gpui::px(6.)),
        checked_shape: ButtonShape::Corner(checked_radius),
    }
}

pub fn resolve_corner_radius(shape: ButtonShape, height: gpui::Pixels) -> gpui::Pixels {
    match shape {
        ButtonShape::CornerFull => height * 0.5,
        ButtonShape::Corner(value) => value,
    }
}

pub struct ToggleButtonColors {
    pub container: Hsla,
    pub content: Hsla,
    pub border: Hsla,
}

pub fn toggle_button_colors(
    variant: ToggleVariant,
    checked: bool,
    cx: &gpui::App,
) -> ToggleButtonColors {
    super::toggle_tokens::colors(variant, checked, cx)
}

#[derive(IntoElement)]
pub struct Toggle {
    id: ElementId,
    style: StyleRefinement,
    checked: bool,
    size: Size,
    variant: ToggleVariant,
    shapes: Option<ToggleButtonShapes>,
    disabled: bool,
    loading: bool,
    border_corners: Corners<bool>,
    border_edges: Edges<bool>,
    children: SmallVec<[AnyElement; 1]>,
    on_click: Option<Rc<dyn Fn(&bool, &mut Window, &mut App) + 'static>>,
    tooltip: ComponentTooltip,
}

impl Toggle {
    /// Create a new Toggle element.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            checked: false,
            size: Size::default(),
            variant: ToggleVariant::default(),
            shapes: None,
            disabled: false,
            loading: false,
            border_corners: Corners {
                top_left: true,
                top_right: true,
                bottom_left: true,
                bottom_right: true,
            },
            border_edges: Edges::all(true),
            children: smallvec![],
            on_click: None,
            tooltip: ComponentTooltip::default(),
        }
    }

    /// Set tooltip text for the toggle.
    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip.text = Some((tooltip.into(), None));
        self
    }

    /// Add a label to the toggle.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        let label: SharedString = label.into();
        self.children.push(label.into_any_element());
        self
    }

    /// Add icon to the toggle.
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        let icon: Icon = icon.into();
        self.children.push(icon.into_any_element());
        self
    }

    /// Set custom ToggleButtonShapes static shapes.
    pub fn shapes(mut self, shapes: ToggleButtonShapes) -> Self {
        self.shapes = Some(shapes);
        self
    }

    /// Set the checked state of the toggle, default: false
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    pub fn on_checked_change(
        self,
        handler: impl Fn(&bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click(handler)
    }

    /// Set the callback to be called when the toggle is clicked.
    ///
    /// The `&bool` parameter represents the new checked state of the toggle.
    pub fn on_click(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }

    pub(crate) fn border_corners(mut self, corners: impl Into<Corners<bool>>) -> Self {
        self.border_corners = corners.into();
        self
    }

    pub(crate) fn border_edges(mut self, edges: impl Into<Edges<bool>>) -> Self {
        self.border_edges = edges.into();
        self
    }
}

impl ToggleVariants for Toggle {
    fn with_variant(mut self, variant: ToggleVariant) -> Self {
        self.variant = variant;
        self
    }
}

impl ParentElement for Toggle {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Disableable for Toggle {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Sizable for Toggle {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl Styled for Toggle {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Toggle {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let metric_tokens = super::toggle_tokens::tokens(self.size);
        let height = metric_tokens.height;
        let resolved_shapes = self
            .shapes
            .unwrap_or_else(|| toggle_button_shapes(self.size));

        let normal_radius = resolve_corner_radius(
            if self.checked {
                resolved_shapes.checked_shape
            } else {
                resolved_shapes.shape
            },
            height,
        );
        let colors = toggle_button_colors(self.variant, self.checked, cx);
        let disabled = self.disabled || self.loading;

        let border_color = if disabled {
            colors
                .border
                .opacity(button_shared_tokens::DISABLED_CONTAINER_OPACITY)
        } else {
            colors.border
        };

        let normal_bg = if disabled {
            if colors.container.a == 0. {
                colors.container
            } else {
                colors
                    .container
                    .opacity(button_shared_tokens::DISABLED_CONTAINER_OPACITY)
            }
        } else {
            colors.container
        };

        let content_color = if disabled {
            colors
                .content
                .opacity(button_shared_tokens::DISABLED_CONTENT_OPACITY)
        } else {
            colors.content
        };

        let focus_handle = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let is_focused = focus_handle.is_focused(window);

        let has_children = !self.children.is_empty();

        let button_el = div()
            .id(self.id.clone())
            .role(Role::CheckBox)
            .aria_toggled(if self.checked {
                gpui::Toggled::True
            } else {
                gpui::Toggled::False
            })
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .h(height)
            .min_w(
                button_dimension_tokens::resolve(self.size, false, false, !has_children).min_width,
            )
            .px(
                button_dimension_tokens::resolve(self.size, false, false, !has_children)
                    .horizontal_padding,
            )
            .py(
                button_dimension_tokens::resolve(self.size, false, false, !has_children)
                    .vertical_padding,
            )
            .when(self.border_corners.top_left, |this| {
                this.rounded_tl(normal_radius)
            })
            .when(self.border_corners.top_right, |this| {
                this.rounded_tr(normal_radius)
            })
            .when(self.border_corners.bottom_left, |this| {
                this.rounded_bl(normal_radius)
            })
            .when(self.border_corners.bottom_right, |this| {
                this.rounded_br(normal_radius)
            })
            .bg(normal_bg)
            .border_color(border_color)
            .when(
                self.variant == ToggleVariant::Outlined || self.variant == ToggleVariant::Outline,
                |this| {
                    this.when(self.border_edges.left, |this| this.border_l_1())
                        .when(self.border_edges.right, |this| this.border_r_1())
                        .when(self.border_edges.top, |this| this.border_t_1())
                        .when(self.border_edges.bottom, |this| this.border_b_1())
                },
            )
            .text_color(content_color)
            .gap(metric_tokens.gap)
            .when(!disabled, |this| {
                this.track_focus(&focus_handle)
                    .cursor(shared::interaction::cursor(
                        false,
                        false,
                        self.style.mouse_cursor,
                    ))
                    .hover(|this| {
                        this.bg(shared::interaction::state_layer(
                            colors.container,
                            colors.content,
                            button_shared_tokens::STATE_HOVER,
                        ))
                    })
                    .active(|this| {
                        this.bg(shared::interaction::state_layer(
                            colors.container,
                            colors.content,
                            button_shared_tokens::STATE_PRESSED,
                        ))
                    })
                    .when(is_focused, |this| {
                        this.bg(shared::interaction::state_layer(
                            colors.container,
                            colors.content,
                            button_shared_tokens::STATE_FOCUS,
                        ))
                    })
            })
            .when(disabled, |this| {
                this.cursor(shared::interaction::cursor(
                    true,
                    self.loading,
                    self.style.mouse_cursor,
                ))
            })
            .when(
                cx.theme().shadow && self.variant == ToggleVariant::Elevated,
                |this| this.shadow_xs(),
            )
            .refine_style(&self.style)
            .cursor(shared::interaction::cursor(
                disabled,
                self.loading,
                self.style.mouse_cursor,
            ))
            .children(self.children);

        let checked = self.checked;
        let on_click = self.on_click.clone();

        button_el
            .when(!disabled, |this| {
                this.on_click(move |_, window, cx| {
                    if let Some(ref handler) = on_click {
                        handler(&!checked, window, cx);
                    }
                })
            })
            .map(|this| self.tooltip.apply(this))
    }
}

/// A group of toggles.
#[derive(IntoElement)]
pub struct ToggleGroup {
    id: ElementId,
    style: StyleRefinement,
    size: Size,
    variant: ToggleVariant,
    disabled: bool,
    segmented: bool,
    items: Vec<Toggle>,
    on_click: Option<Rc<dyn Fn(&Vec<bool>, &mut Window, &mut App) + 'static>>,
}

impl ToggleGroup {
    /// Create a new ToggleGroup element.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            size: Size::default(),
            variant: ToggleVariant::default(),
            disabled: false,
            segmented: false,
            items: Vec::new(),
            on_click: None,
        }
    }

    /// Add a child [`Toggle`] to the group.
    pub fn child(mut self, toggle: impl Into<Toggle>) -> Self {
        self.items.push(toggle.into());
        self
    }

    /// Add multiple [`Toggle`]s to the group.
    pub fn children(mut self, children: impl IntoIterator<Item = impl Into<Toggle>>) -> Self {
        self.items.extend(children.into_iter().map(Into::into));
        self
    }

    /// Set the callback to be called when the toggle group changes.
    ///
    /// The `&Vec<bool>` parameter represents the new check state of each [`Toggle`] in the group.
    pub fn on_click(
        mut self,
        on_click: impl Fn(&Vec<bool>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(on_click));
        self
    }

    /// Render the group as a connected segmented control.
    ///
    /// This keeps the existing multi-toggle behavior, but removes the default
    /// gap and joins adjacent item borders into a single segmented outline.
    pub fn segmented(mut self) -> Self {
        self.segmented = true;
        self
    }
}

impl Sizable for ToggleGroup {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl ToggleVariants for ToggleGroup {
    fn with_variant(mut self, variant: ToggleVariant) -> Self {
        self.variant = variant;
        self
    }
}

impl Disableable for ToggleGroup {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Styled for ToggleGroup {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for ToggleGroup {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let disabled = self.disabled;
        let items_len = self.items.len();
        let checks = self
            .items
            .iter()
            .map(|item| item.checked)
            .collect::<Vec<bool>>();
        let state = Rc::new(Cell::new(None));

        h_flex()
            .id(self.id)
            .role(Role::Toolbar)
            .when(!self.segmented, |this| this.gap_2())
            .refine_style(&self.style)
            .children(self.items.into_iter().enumerate().map({
                |(ix, item)| {
                    let state = state.clone();
                    let item = if !self.segmented || items_len == 1 {
                        item
                    } else if ix == 0 {
                        item.border_corners(Corners {
                            top_left: true,
                            top_right: false,
                            bottom_left: true,
                            bottom_right: false,
                        })
                        .border_edges(Edges {
                            left: true,
                            top: true,
                            right: true,
                            bottom: true,
                        })
                    } else if ix == items_len - 1 {
                        item.border_corners(Corners {
                            top_left: false,
                            top_right: true,
                            bottom_left: false,
                            bottom_right: true,
                        })
                        .border_edges(Edges {
                            left: false,
                            top: true,
                            right: true,
                            bottom: true,
                        })
                    } else {
                        item.border_corners(Corners {
                            top_left: false,
                            top_right: false,
                            bottom_left: false,
                            bottom_right: false,
                        })
                        .border_edges(Edges {
                            left: false,
                            top: true,
                            right: true,
                            bottom: true,
                        })
                    };

                    item.disabled(disabled)
                        .with_size(self.size)
                        .with_variant(self.variant)
                        .on_click(move |_, _, _| {
                            state.set(Some(ix));
                        })
                }
            }))
            .when(!disabled, |this| {
                this.when_some(self.on_click, |this, on_click| {
                    this.on_click(move |_, window, cx| {
                        if let Some(ix) = state.get() {
                            let mut checks = checks.clone();
                            checks[ix] = !checks[ix];
                            on_click(&checks, window, cx);
                        }
                    })
                })
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IconName;
    use gpui::{
        AppContext, Context, Entity, IntoElement, Render, TestAppContext, VisualTestContext,
        Window, div, px,
    };

    struct ToggleState {
        clicks: usize,
    }

    struct ToggleRoot {
        state: Entity<ToggleState>,
        disabled: bool,
        loading: bool,
    }

    impl Render for ToggleRoot {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let state = self.state.clone();
            let checked = self.state.read(cx).clicks > 0;
            div().size_full().child(
                div()
                    .debug_selector(|| "toggle-button".to_string())
                    .size(px(48.))
                    .child(
                        Toggle::new("toggle-button")
                            .checked(checked)
                            .disabled(self.disabled)
                            .loading(self.loading)
                            .on_click(move |_, _, cx| {
                                state.update(cx, |state, _| state.clicks += 1);
                            }),
                    ),
            )
        }
    }

    #[gpui::test]
    fn test_toggle_builder(_cx: &mut gpui::TestAppContext) {
        let toggle = Toggle::new("complex-toggle")
            .label("Enable Feature")
            .icon(IconName::Check)
            .checked(true)
            .outline()
            .large()
            .disabled(false)
            .on_click(|_, _, _| {});

        assert_eq!(toggle.children.len(), 2); // label + icon
        assert!(toggle.checked);
        assert_eq!(toggle.variant, ToggleVariant::Outline);
        assert_eq!(toggle.size, Size::Large);
        assert!(!toggle.disabled);
        assert!(toggle.on_click.is_some());
    }

    #[test]
    fn toggle_shape_table_uses_static_unchecked_and_checked_shapes() {
        let expected = [
            (Size::XSmall, 8.),
            (Size::Small, 8.),
            (Size::Medium, 12.),
            (Size::Large, 16.),
        ];
        for (size, checked_radius) in expected {
            let shapes = toggle_button_shapes(size);
            assert_eq!(shapes.shape, ButtonShape::CornerFull);
            assert_eq!(
                shapes.checked_shape,
                ButtonShape::Corner(px(checked_radius))
            );
            assert_eq!(shapes.pressed_shape, ButtonShape::Corner(px(6.)));
        }
    }

    #[gpui::test]
    fn test_toggle_group_builder(_cx: &mut gpui::TestAppContext) {
        let group = ToggleGroup::new("complex-group")
            .child(Toggle::new("toggle1").label("Option 1"))
            .child(Toggle::new("toggle2").label("Option 2").checked(true))
            .child(Toggle::new("toggle3").label("Option 3"))
            .outline()
            .large()
            .segmented()
            .disabled(false)
            .on_click(|_, _, _| {});

        assert_eq!(group.items.len(), 3);
        assert_eq!(group.variant, ToggleVariant::Outline);
        assert_eq!(group.size, Size::Large);
        assert!(group.segmented);
        assert!(!group.disabled);
        assert!(group.on_click.is_some());
    }

    #[gpui::test]
    fn rendered_toggle_click_updates_entity_state(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let state = cx.new(|_| ToggleState { clicks: 0 });
        let state_for_root = state.clone();
        let (_, visual) = cx.add_window_view(move |_, _| ToggleRoot {
            state: state_for_root,
            disabled: false,
            loading: false,
        });
        let visual: &mut VisualTestContext = visual;
        visual.run_until_parked();
        visual.update(|window, cx| {
            _ = window.draw(cx);
        });
        let bounds = visual.debug_bounds("toggle-button").expect("toggle bounds");
        visual.simulate_click(bounds.center(), Default::default());
        assert_eq!(state.read_with(visual, |state, _| state.clicks), 1);
    }

    #[gpui::test]
    fn rendered_toggle_disabled_and_loading_suppress_callbacks(cx: &mut TestAppContext) {
        for (disabled, loading) in [(true, false), (false, true)] {
            cx.update(crate::init);
            let state = cx.new(|_| ToggleState { clicks: 0 });
            let state_for_root = state.clone();
            let (_, visual) = cx.add_window_view(move |_, _| ToggleRoot {
                state: state_for_root,
                disabled,
                loading,
            });
            let visual: &mut VisualTestContext = visual;
            visual.run_until_parked();
            visual.update(|window, cx| _ = window.draw(cx));
            let bounds = visual.debug_bounds("toggle-button").unwrap();
            visual.simulate_click(bounds.center(), Default::default());
            assert_eq!(state.read_with(visual, |state, _| state.clicks), 0);
        }
    }

    #[gpui::test]
    fn rendered_toggle_pressed_bounds_remain_static(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let state = cx.new(|_| ToggleState { clicks: 0 });
        let state_for_root = state.clone();
        let (_, visual) = cx.add_window_view(move |_, _| ToggleRoot {
            state: state_for_root,
            disabled: false,
            loading: false,
        });
        let visual: &mut VisualTestContext = visual;
        visual.run_until_parked();
        visual.update(|window, cx| _ = window.draw(cx));
        let before = visual.debug_bounds("toggle-button").unwrap();
        visual.simulate_mouse_down(before.center(), gpui::MouseButton::Left, Default::default());
        visual.simulate_mouse_up(before.center(), gpui::MouseButton::Left, Default::default());
        assert_eq!(before, visual.debug_bounds("toggle-button").unwrap());
    }
}
