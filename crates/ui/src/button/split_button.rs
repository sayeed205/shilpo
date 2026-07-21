use gpui::{
    Anchor, Animation, AnimationExt as _, App, Context, Corners, ElementId, InteractiveElement,
    IntoElement, ParentElement, RenderOnce, SharedString, StyleRefinement, Styled, Window, div,
    prelude::FluentBuilder,
};
use std::time::Duration;
use std::{cell::Cell, rc::Rc};

use crate::{
    Disableable, Selectable, Sizable, Size, StyledExt as _,
    menu::{DropdownMenu, PopupMenu},
    tooltip::ComponentTooltip,
};

use super::{
    Button, ButtonRounded, ButtonVariant, ButtonVariants, SplitButtonShapes, split_button_tokens,
};

#[derive(IntoElement)]
pub struct SplitButton {
    id: ElementId,
    style: StyleRefinement,
    leading: Button,
    trailing: Button,
    variant: ButtonVariant,
    size: Size,
    disabled: bool,
    loading: bool,
    compact: bool,
    outline: bool,
    rounded: ButtonRounded,
    spacing: Option<gpui::Pixels>,
    shapes: Option<SplitButtonShapes>,
    anchor: Anchor,
    menu:
        Option<Rc<dyn Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static>>,
    tooltip: ComponentTooltip,
}

impl SplitButton {
    /// Create a new SplitButton.
    pub fn new(id: impl Into<ElementId>, leading: Button, trailing: Button) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            leading,
            trailing,
            variant: ButtonVariant::Filled,
            size: Size::Medium,
            disabled: false,
            loading: false,
            compact: false,
            outline: false,
            rounded: ButtonRounded::Token,
            spacing: None,
            shapes: None,
            // TopRight anchors popover top-right to trigger top-right: below and right-aligned.
            anchor: Anchor::TopRight,
            menu: None,
            tooltip: ComponentTooltip::default(),
        }
    }

    /// Creates a new tonal SplitButton.
    pub fn tonal(id: impl Into<ElementId>, leading: Button, trailing: Button) -> Self {
        Self::new(id, leading, trailing).with_variant(ButtonVariant::FilledTonal)
    }

    /// Creates a new outlined SplitButton.
    pub fn outlined(id: impl Into<ElementId>, leading: Button, trailing: Button) -> Self {
        Self::new(id, leading, trailing).with_variant(ButtonVariant::Outlined)
    }

    /// Creates a new elevated SplitButton.
    pub fn elevated(id: impl Into<ElementId>, leading: Button, trailing: Button) -> Self {
        Self::new(id, leading, trailing).with_variant(ButtonVariant::Elevated)
    }

    /// Sets the button to compact style.
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    /// Sets the button to outline style.
    pub fn outline(mut self) -> Self {
        self.outline = true;
        self
    }

    /// Sets the rounded style of the split button.
    pub fn rounded(mut self, rounded: impl Into<ButtonRounded>) -> Self {
        self.rounded = rounded.into();
        self
    }

    pub fn spacing(mut self, spacing: gpui::Pixels) -> Self {
        self.spacing = Some(spacing);
        self
    }

    pub fn shapes(mut self, shapes: SplitButtonShapes) -> Self {
        self.shapes = Some(shapes);
        self
    }

    pub fn shape_tokens(&self) -> SplitButtonShapes {
        self.shapes
            .unwrap_or_else(|| split_button_tokens::tokens(self.size).shapes)
    }

    /// Sets the loading state.
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    /// Sets the dropdown menu for the trailing half.
    pub fn dropdown_menu(
        mut self,
        menu: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> Self {
        self.menu = Some(Rc::new(menu));
        self
    }

    /// Sets the dropdown menu for the trailing half with a custom anchor corner.
    pub fn dropdown_menu_with_anchor(
        mut self,
        anchor: impl Into<Anchor>,
        menu: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> Self {
        self.menu = Some(Rc::new(menu));
        self.anchor = anchor.into();
        self
    }

    /// Sets the tooltip text for the split button.
    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip.text = Some((tooltip.into(), None));
        self
    }
}

impl Disableable for SplitButton {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Styled for SplitButton {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Sizable for SplitButton {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl ButtonVariants for SplitButton {
    fn with_variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }
}

#[derive(IntoElement)]
pub(crate) struct SplitButtonTrailingTrigger {
    button: Button,
    outer_radius: gpui::Pixels,
    trailing_corners: Corners<gpui::Pixels>,
    height: gpui::Pixels,
    selected: bool,
}

#[derive(Clone)]
struct CornerMorphState {
    target: bool,
    from: Corners<gpui::Pixels>,
    current: Rc<Cell<Corners<gpui::Pixels>>>,
    active_generation: Rc<Cell<u64>>,
    generation: u64,
    active: bool,
}

impl Styled for SplitButtonTrailingTrigger {
    fn style(&mut self) -> &mut StyleRefinement {
        self.button.style()
    }
}

impl InteractiveElement for SplitButtonTrailingTrigger {
    fn interactivity(&mut self) -> &mut gpui::Interactivity {
        self.button.interactivity()
    }
}

impl DropdownMenu for SplitButtonTrailingTrigger {}

impl Selectable for SplitButtonTrailingTrigger {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl RenderOnce for SplitButtonTrailingTrigger {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let target = self.selected;
        let previous_key = self
            .button
            .interactivity()
            .element_id
            .clone()
            .map(|id| format!("split-button-morph:{id:?}"))
            .unwrap_or_else(|| "split-button-morph".into());
        let closed_corners = self.trailing_corners;
        let open_corners = Corners::all(self.outer_radius);
        let morph = window.use_keyed_state(previous_key, cx, |_, _| CornerMorphState {
            target,
            from: if target { open_corners } else { closed_corners },
            current: Rc::new(Cell::new(if target {
                open_corners
            } else {
                closed_corners
            })),
            active_generation: Rc::new(Cell::new(0)),
            generation: 0,
            active: false,
        });
        let state = morph.read(cx).clone();
        let changed = state.target != target;
        let generation = if changed {
            let generation = state.generation.wrapping_add(1);
            let from = state.current.get();
            morph.update(cx, |state, _| {
                state.target = target;
                state.from = from;
                state.generation = generation;
                state.active = true;
                state.current.set(from);
                state.active_generation.set(generation);
            });
            let morph = morph.clone();
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                _ = morph.update(cx, |state, cx| {
                    if state.generation == generation {
                        state.active = false;
                        state
                            .current
                            .set(if target { open_corners } else { closed_corners });
                        cx.notify();
                    }
                });
            })
            .detach();
            generation
        } else {
            state.generation
        };
        let state = morph.read(cx);
        let active_corners = if target { open_corners } else { closed_corners };
        let active_rotation = if target { std::f32::consts::PI } else { 0. };

        let button = self
            .button
            .rounded(ButtonRounded::Size(self.height))
            .corner_radii(active_corners)
            .when(target, |this| this.w(self.height))
            .dropdown_caret_rotation(gpui::Radians(active_rotation))
            .selected(target);

        if state.active {
            let from = state.from;
            let to = if target { open_corners } else { closed_corners };
            let current = state.current.clone();
            let active_generation = state.active_generation.clone();
            let animation = Animation::new(Duration::from_millis(250))
                .with_easing(crate::animation::cubic_bezier(0.2, 0.0, 0.0, 1.0));
            return button
                .with_animation(
                    format!("split-button-corner-morph:{generation}"),
                    animation,
                    move |button, delta| {
                        let corners = crate::motion::lerp_corners(from, to, delta);
                        if active_generation.get() == generation {
                            current.set(corners);
                        }
                        button
                            .corner_radii(corners)
                            .dropdown_caret_rotation(gpui::Radians(if target {
                                std::f32::consts::PI * delta
                            } else {
                                std::f32::consts::PI * (1. - delta)
                            }))
                    },
                )
                .into_any_element();
        }

        button.into_any_element()
    }
}

impl RenderOnce for SplitButton {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let tokens = split_button_tokens::tokens(self.size);
        let height = tokens.height;
        let (leading_left, leading_inner, trailing_inner, trailing_right) = (
            tokens.leading_start,
            tokens.leading_end,
            tokens.trailing_start,
            tokens.trailing_end,
        );

        let leading_left = if self.compact {
            leading_left * 0.5
        } else {
            leading_left
        };
        let leading_inner = if self.compact {
            leading_inner * 0.5
        } else {
            leading_inner
        };
        let trailing_inner = if self.compact {
            trailing_inner * 0.5
        } else {
            trailing_inner
        };
        let trailing_right = if self.compact {
            trailing_right * 0.5
        } else {
            trailing_right
        };

        let variant = if self.outline {
            ButtonVariant::Outlined
        } else {
            self.variant
        };
        let outer_radius =
            crate::button::button_shape_tokens::resolve(self.rounded, self.size, Some(height));
        let leading_corners = gpui::Corners {
            top_left: outer_radius,
            bottom_left: outer_radius,
            top_right: tokens.inner_radius,
            bottom_right: tokens.inner_radius,
        };
        let trailing_corners = gpui::Corners {
            top_left: tokens.inner_radius,
            bottom_left: tokens.inner_radius,
            top_right: outer_radius,
            bottom_right: outer_radius,
        };

        let leading = self
            .leading
            .with_variant(variant)
            .with_size(self.size)
            .disabled(self.disabled || self.loading)
            .loading(self.loading)
            .h(height)
            .rounded(ButtonRounded::Size(height))
            .corner_radii(leading_corners)
            .border_corners(Corners {
                top_left: true,
                top_right: true,
                bottom_left: true,
                bottom_right: true,
            })
            .pl(leading_left)
            .pr(leading_inner)
            .min_w(tokens.min_width);

        let trailing_element = if let Some(menu) = self.menu {
            let menu = move |pop: PopupMenu,
                             win: &mut Window,
                             ctx: &mut Context<PopupMenu>|
                  -> PopupMenu { (menu)(pop, win, ctx) };

            let trigger = SplitButtonTrailingTrigger {
                button: self
                    .trailing
                    .with_variant(variant)
                    .with_size(self.size)
                    .disabled(self.disabled || self.loading)
                    .loading(self.loading)
                    .pressed_corner_shape(false)
                    .dropdown_caret_size(tokens.icon)
                    .h(height)
                    .border_corners(Corners {
                        top_left: true,
                        top_right: true,
                        bottom_left: true,
                        bottom_right: true,
                    })
                    .pl(trailing_inner)
                    .pr(trailing_right)
                    .min_w(tokens.min_width),
                outer_radius,
                trailing_corners,
                height,
                selected: false,
            };

            trigger
                .dropdown_menu_with_anchor(self.anchor, menu)
                .into_any_element()
        } else {
            self.trailing
                .with_variant(variant)
                .with_size(self.size)
                .disabled(self.disabled || self.loading)
                .loading(self.loading)
                .h(height)
                .border_corners(Corners {
                    top_left: true,
                    top_right: true,
                    bottom_left: true,
                    bottom_right: true,
                })
                .pl(trailing_inner)
                .pr(trailing_right)
                .min_w(tokens.min_width)
                .rounded(ButtonRounded::Size(height))
                .corner_radii(trailing_corners)
                .into_any_element()
        };

        div()
            .id(self.id)
            .h_flex()
            .gap(self.spacing.unwrap_or(tokens.between_space))
            .cursor(super::shared::interaction::cursor(
                self.disabled,
                self.loading,
                self.style.mouse_cursor,
            ))
            .refine_style(&self.style)
            .cursor(super::shared::interaction::cursor(
                self.disabled,
                self.loading,
                self.style.mouse_cursor,
            ))
            .child(leading)
            .child(trailing_element)
            .map(|this| self.tooltip.apply(this))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;

    #[gpui::test]
    fn test_split_button_builder(_cx: &mut gpui::TestAppContext) {
        let leading = Button::new("lead").label("Lead");
        let trailing = Button::new("trail").label("Trail");
        let split = SplitButton::tonal("split", leading, trailing)
            .outline()
            .large()
            .compact()
            .loading(false)
            .disabled(false)
            .rounded(ButtonRounded::Medium)
            .dropdown_menu_with_anchor(Anchor::BottomLeft, |menu, _, _| menu);

        assert_eq!(split.variant, ButtonVariant::FilledTonal);
        assert!(split.outline);
        assert_eq!(split.size, Size::Large);
        assert!(split.compact);
        assert!(!split.loading);
        assert!(!split.disabled);
        assert!(matches!(split.rounded, ButtonRounded::Medium));
        assert!(split.menu.is_some());
        assert_eq!(split.anchor, Anchor::BottomLeft);
    }

    #[test]
    fn split_button_spacing_and_shape_override_are_controlled() {
        let shapes = split_button_tokens::tokens(Size::Medium).shapes;
        let split = SplitButton::new("split", Button::new("lead"), Button::new("trail"))
            .spacing(px(6.))
            .shapes(shapes)
            .disabled(true)
            .loading(true);
        assert_eq!(split.spacing, Some(px(6.)));
        assert_eq!(split.shape_tokens(), shapes);
        assert!(split.disabled);
        assert!(split.loading);
    }

    #[test]
    fn split_button_default_dropdown_anchor_is_below_and_right_aligned() {
        let split = SplitButton::new("split", Button::new("lead"), Button::new("trail"));

        assert_eq!(split.anchor, Anchor::TopRight);
    }
}
