use std::rc::Rc;

use gpui::{
    AbsoluteLength, AnyElement, App, Background, ClickEvent, Corners, CursorStyle, DefiniteLength,
    Div, Edges, ElementId, Hsla, InteractiveElement, Interactivity, IntoElement, Length,
    MouseButton, ParentElement, Pixels, RenderOnce, Role, SharedString, Stateful,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window, div,
    prelude::FluentBuilder as _, px, relative,
};

use super::{
    button_dimension_tokens,
    button_geometry::ButtonSlotGeometry,
    button_geometry::{self, CornerShape, CornerToken},
    button_shape_tokens, button_shared_tokens, button_tokens, shared,
};
use crate::{
    ActiveTheme, Disableable, FocusableExt as _, Icon, IconName, Selectable, Sizable, Size,
    StyleSized, StyledExt,
    button::ButtonIcon,
    h_flex,
    tooltip::{ManagedTooltipExt as _, Tooltip},
};

#[derive(Default, Clone, Copy)]
pub enum ButtonRounded {
    #[default]
    Token,
    None,
    Small,
    Medium,
    Large,
    Size(Pixels),
}

impl From<Pixels> for ButtonRounded {
    fn from(px: Pixels) -> Self {
        ButtonRounded::Size(px)
    }
}

pub trait ButtonVariants: Sized {
    fn with_variant(self, variant: ButtonVariant) -> Self;
    fn filled(self) -> Self {
        self.with_variant(ButtonVariant::Filled)
    }
    fn elevated(self) -> Self {
        self.with_variant(ButtonVariant::Elevated)
    }
    fn filled_tonal(self) -> Self {
        self.with_variant(ButtonVariant::FilledTonal)
    }
    fn outlined(self) -> Self {
        self.with_variant(ButtonVariant::Outlined)
    }
    fn text(self) -> Self {
        self.with_variant(ButtonVariant::Text)
    }
    fn plain(self) -> Self {
        self.with_variant(ButtonVariant::Plain)
    }
}

/// The variant of the Button.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum ButtonVariant {
    #[default]
    Filled,
    Elevated,
    FilledTonal,
    Outlined,
    Text,
    Plain,
}

/// A Button element.
#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    base: Stateful<Div>,
    style: StyleRefinement,
    icon: Option<ButtonIcon>,
    label: Option<SharedString>,
    children: Vec<AnyElement>,
    disabled: bool,
    pub(crate) selected: bool,
    variant: ButtonVariant,
    rounded: ButtonRounded,
    corner_radii: Option<Corners<Pixels>>,
    slot_geometry: Option<ButtonSlotGeometry>,
    outline: bool,
    border_corners: Corners<bool>,
    border_edges: Edges<bool>,
    dropdown_caret: bool,
    size: Size,
    compact: bool,
    tooltip: Option<(
        SharedString,
        Option<(Rc<Box<dyn gpui::Action>>, Option<SharedString>)>,
    )>,
    tooltip_builder: Option<Rc<dyn Fn(&mut Window, &mut App) -> gpui::AnyView>>,
    on_click: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
    on_hover: Option<Rc<dyn Fn(&bool, &mut Window, &mut App)>>,
    loading: bool,
    pressed_corner_shape: bool,
    dropdown_caret_rotation: Option<gpui::Radians>,
    dropdown_caret_size: Option<Pixels>,
    loading_icon: Option<Icon>,
    full_width: bool,
    pl: Option<gpui::Pixels>,
    pr: Option<gpui::Pixels>,

    tab_index: isize,
    tab_stop: bool,
}

impl From<Button> for AnyElement {
    fn from(button: Button) -> Self {
        button.into_any_element()
    }
}

impl Button {
    pub fn new(id: impl Into<ElementId>) -> Self {
        let id = id.into();

        Self {
            id: id.clone(),
            // ID must be set after div is created;
            // `dropdown_menu` uses this id to create the popup menu.
            base: div().flex_shrink_0().id(id),
            style: StyleRefinement::default(),
            icon: None,
            label: None,
            disabled: false,
            selected: false,
            variant: ButtonVariant::default(),
            rounded: ButtonRounded::default(),
            corner_radii: None,
            slot_geometry: None,
            border_corners: Corners {
                top_left: true,
                top_right: true,
                bottom_right: true,
                bottom_left: true,
            },
            border_edges: Edges::all(true),
            size: Size::Small,
            tooltip: None,
            tooltip_builder: None,
            on_click: None,
            on_hover: None,
            loading: false,
            pressed_corner_shape: true,
            dropdown_caret_rotation: None,
            dropdown_caret_size: None,
            compact: false,
            outline: false,
            children: Vec::new(),
            loading_icon: None,
            full_width: false,
            dropdown_caret: false,
            pl: None,
            pr: None,
            tab_index: 0,
            tab_stop: true,
        }
    }

    #[cfg(test)]
    pub(crate) fn test_debug_selector(self, selector: &'static str) -> Self {
        Self {
            base: self.base.debug_selector(move || selector.to_string()),
            ..self
        }
    }

    /// Override default pointing-hand cursor for this Button.
    ///
    /// Applied after Button defaults, so caller choice wins.
    pub fn cursor(mut self, cursor: CursorStyle) -> Self {
        self.style.mouse_cursor = Some(cursor);
        self
    }

    /// Set the outline style of the Button.
    pub fn outline(mut self) -> Self {
        self.outline = true;
        self
    }

    /// Set the border radius of the Button.
    pub fn rounded(mut self, rounded: impl Into<ButtonRounded>) -> Self {
        self.rounded = rounded.into();
        self
    }

    /// Set the border corners side of the Button.
    pub(crate) fn border_corners(mut self, corners: impl Into<Corners<bool>>) -> Self {
        self.border_corners = corners.into();
        self
    }

    pub(crate) fn corner_radii(mut self, radii: Corners<Pixels>) -> Self {
        self.corner_radii = Some(radii);
        self
    }

    /// Opt out of pressed-state corner interpolation while retaining other press feedback.
    pub(crate) fn pressed_corner_shape(mut self, enabled: bool) -> Self {
        self.pressed_corner_shape = enabled;
        self
    }

    pub(crate) fn dropdown_caret_rotation(mut self, rotation: gpui::Radians) -> Self {
        self.dropdown_caret_rotation = Some(rotation);
        self
    }

    pub(crate) fn dropdown_caret_size(mut self, size: Pixels) -> Self {
        self.dropdown_caret_size = Some(size);
        self
    }

    /// Terminal geometry supplied by compound controls. Applied after user style refinement.
    #[allow(dead_code)]
    pub(crate) fn slot_geometry(mut self, geometry: ButtonSlotGeometry) -> Self {
        self.slot_geometry = Some(geometry);
        self
    }

    /// Set the border edges of the Button.
    #[allow(dead_code)]
    pub(crate) fn border_edges(mut self, edges: impl Into<Edges<bool>>) -> Self {
        self.border_edges = edges.into();
        self
    }

    /// Set label to the Button, if no label is set, the button will be in Icon Button mode.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set the icon of the button, if the Button have no label, the button well in Icon Button mode.
    pub fn icon(mut self, icon: impl Into<ButtonIcon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn pl(mut self, pl: impl Into<gpui::Pixels>) -> Self {
        self.pl = Some(pl.into());
        self
    }

    pub fn pr(mut self, pr: impl Into<gpui::Pixels>) -> Self {
        self.pr = Some(pr.into());
        self
    }

    /// Set the tooltip of the button.
    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some((tooltip.into(), None));
        self
    }

    /// Set the tooltip of the button with action to show keybinding.
    pub fn tooltip_with_action(
        mut self,
        tooltip: impl Into<SharedString>,
        action: &dyn gpui::Action,
        context: Option<&str>,
    ) -> Self {
        self.tooltip = Some((
            tooltip.into(),
            Some((
                Rc::new(action.boxed_clone()),
                context.map(|c| c.to_string().into()),
            )),
        ));
        self
    }

    /// Set true to show the loading indicator.
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    /// Set true to make the button take up the full width of its parent container.
    pub fn full_width(mut self, full_width: bool) -> Self {
        self.full_width = full_width;
        self
    }

    /// Set the button to compact mode, then padding will be reduced.
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    /// Add click handler.
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }

    /// Add hover handler, the bool parameter indicates whether the mouse is hovering.
    pub fn on_hover(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_hover = Some(Rc::new(handler));
        self
    }

    /// Set the loading icon of the button, it will be used when loading is true.
    ///
    /// Default is a spinner icon.
    pub fn loading_icon(mut self, icon: impl Into<Icon>) -> Self {
        self.loading_icon = Some(icon.into());
        self
    }

    /// Set the tab index of the button, it will be used to focus the button by tab key.
    ///
    /// Default is 0.
    pub fn tab_index(mut self, tab_index: isize) -> Self {
        self.tab_index = tab_index;
        self
    }

    /// Set the tab stop of the button, if true, the button will be focusable by tab key.
    ///
    /// Default is true.
    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }

    /// Set to show a dropdown caret icon at the end of the button.
    pub fn dropdown_caret(mut self, dropdown_caret: bool) -> Self {
        self.dropdown_caret = dropdown_caret;
        self
    }

    #[inline]
    fn clickable(&self) -> bool {
        !(self.disabled || self.loading) && self.on_click.is_some()
    }

    #[inline]
    fn hoverable(&self) -> bool {
        !(self.disabled || self.loading) && self.on_hover.is_some()
    }
}

impl Disableable for Button {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Selectable for Button {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl Sizable for Button {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl ButtonVariants for Button {
    fn with_variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }
}

impl Styled for Button {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Button {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl InteractiveElement for Button {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl RenderOnce for Button {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let style: ButtonVariant = if self.outline {
            ButtonVariant::Outlined
        } else {
            self.variant
        };
        #[cfg(test)]
        button_tokens::record_render_paint(button_tokens::RenderPaintCapture {
            variant: style,
            base: button_tokens::resolved_paint(style, button_tokens::ButtonPaintState::Rest, cx),
            hover: button_tokens::resolved_paint(style, button_tokens::ButtonPaintState::Hover, cx),
            focus: button_tokens::resolved_paint(style, button_tokens::ButtonPaintState::Focus, cx),
            pressed: button_tokens::resolved_paint(
                style,
                button_tokens::ButtonPaintState::Pressed,
                cx,
            ),
            disabled: button_tokens::resolved_paint(
                style,
                button_tokens::ButtonPaintState::Disabled,
                cx,
            ),
        });
        let clickable = self.clickable();
        let is_disabled = self.disabled;
        let cursor_disabled = self.disabled || self.loading;
        let hoverable = self.hoverable();
        let normal_style = style.normal(cx);
        let icon_only = self.label.is_none() && self.children.is_empty() && self.icon.is_some();
        let dimensions =
            button_dimension_tokens::resolve(self.size, self.variant, self.compact, icon_only);

        let focus_handle = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let is_focused = focus_handle.is_focused(window);

        let ripple_state = window.use_keyed_state(format!("{}-ripple", self.id), cx, |_, _| {
            crate::ripple::RippleState::new()
        });

        let rounding =
            button_shape_tokens::resolve(self.rounded, self.size, Some(dimensions.height));
        let token = if matches!(self.rounded, ButtonRounded::Token) {
            CornerToken::Full
        } else {
            CornerToken::Fixed(rounding)
        };
        let explicit = self.corner_radii.map(|radii| CornerShape {
            top_left: CornerToken::Fixed(radii.top_left),
            top_right: CornerToken::Fixed(radii.top_right),
            bottom_right: CornerToken::Fixed(radii.bottom_right),
            bottom_left: CornerToken::Fixed(radii.bottom_left),
        });
        let shape = explicit.unwrap_or(CornerShape::all(token));
        let shape = CornerShape {
            top_left: if self.border_corners.top_left {
                shape.top_left
            } else {
                CornerToken::Fixed(Pixels::ZERO)
            },
            top_right: if self.border_corners.top_right {
                shape.top_right
            } else {
                CornerToken::Fixed(Pixels::ZERO)
            },
            bottom_right: if self.border_corners.bottom_right {
                shape.bottom_right
            } else {
                CornerToken::Fixed(Pixels::ZERO)
            },
            bottom_left: if self.border_corners.bottom_left {
                shape.bottom_left
            } else {
                CornerToken::Fixed(Pixels::ZERO)
            },
        };
        let effective_height = match self.style.size.height {
            Some(Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(value)))) => {
                value
            }
            _ => dimensions.height,
        };
        let resolved_geometry = button_geometry::assemble(
            effective_height,
            dimensions.min_width,
            Edges {
                left: self.pl.unwrap_or(dimensions.horizontal_padding),
                right: self.pr.unwrap_or(dimensions.horizontal_padding),
                top: dimensions.vertical_padding,
                bottom: dimensions.vertical_padding,
            },
            shape,
            self.border_edges,
            self.slot_geometry,
        );
        #[cfg(test)]
        button_geometry::record_render_geometry(resolved_geometry);
        let terminal_geometry = self.slot_geometry.map(|_| resolved_geometry);
        let radii = resolved_geometry.corners;
        let spring_progress = ripple_state.read(cx).current_spring_progress();
        let pressed_rounding =
            button_shape_tokens::resolve_pressed(self.rounded, self.size, Some(dimensions.height));
        let pressed_radii = Corners::all(pressed_rounding);
        let active_radii = if self.pressed_corner_shape && spring_progress > 0.0 && !self.disabled {
            crate::motion::lerp_corners(radii, pressed_radii, spring_progress)
        } else {
            radii
        };

        let button_element = self
            .base
            .role(Role::Button)
            .when_some(self.label.as_ref(), |this, label| {
                this.aria_label(label.clone())
            })
            .aria_selected(self.selected)
            .when(!self.disabled, |this| {
                this.track_focus(
                    &focus_handle
                        .tab_index(self.tab_index)
                        .tab_stop(self.tab_stop),
                )
            })
            .cursor(shared::interaction::cursor(
                self.disabled,
                self.loading,
                None,
            ))
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .when(dimensions.height > Pixels::ZERO, |this| {
                this.h(dimensions.height)
            })
            .when(self.full_width, |this| this.w_full())
            .when(
                !self.full_width && dimensions.min_width > Pixels::ZERO,
                |this| this.min_w(dimensions.min_width),
            )
            .when_some(self.pl, |this, pl| this.pl(pl))
            .when_some(self.pr, |this, pr| this.pr(pr))
            .when(
                self.pl.is_none()
                    && self.pr.is_none()
                    && dimensions.horizontal_padding > Pixels::ZERO,
                |this| this.px(dimensions.horizontal_padding),
            )
            .when(dimensions.vertical_padding > Pixels::ZERO, |this| {
                this.py(dimensions.vertical_padding)
            })
            .when(cx.theme().shadow && normal_style.shadow > 0, |this| {
                if normal_style.shadow > 1 {
                    this.shadow_md()
                } else {
                    this.shadow_xs()
                }
            })
            .rounded_tl(active_radii.top_left)
            .rounded_tr(active_radii.top_right)
            .rounded_bl(active_radii.bottom_left)
            .rounded_br(active_radii.bottom_right)
            .when(style == ButtonVariant::Outlined, |this| {
                this.when(self.border_edges.left, |this| {
                    this.border_l(dimensions.outline)
                })
                .when(!self.border_edges.left, |this| this.border_l(px(0.)))
                .when(self.border_edges.right, |this| {
                    this.border_r(dimensions.outline)
                })
                .when(!self.border_edges.right, |this| this.border_r(px(0.)))
                .when(self.border_edges.top, |this| {
                    this.border_t(dimensions.outline)
                })
                .when(!self.border_edges.top, |this| this.border_t(px(0.)))
                .when(self.border_edges.bottom, |this| {
                    this.border_b(dimensions.outline)
                })
                .when(!self.border_edges.bottom, |this| this.border_b(px(0.)))
            })
            .text_color(normal_style.fg)
            .when(self.selected, |this| {
                let selected_style = style.selected(cx);
                this.bg(selected_style.bg)
                    .border_color(selected_style.border)
                    .text_color(selected_style.fg)
            })
            .when(!self.disabled && !self.selected, |this| {
                this.border_color(normal_style.border)
                    .bg(normal_style.bg)
                    .when(normal_style.underline, |this| this.text_decoration_1())
                    .hover(|this| {
                        let hover_style = style.hovered(cx);
                        let this = this
                            .bg(hover_style.bg)
                            .border_color(hover_style.border)
                            .text_color(hover_style.fg);
                        if cx.theme().shadow && hover_style.shadow > 0 {
                            if hover_style.shadow > 1 {
                                this.shadow_md()
                            } else {
                                this.shadow_xs()
                            }
                        } else {
                            this
                        }
                    })
                    .active(|this| {
                        let active_style = style.pressed(cx);
                        let this = this
                            .bg(active_style.bg)
                            .border_color(active_style.border)
                            .text_color(active_style.fg);
                        if cx.theme().shadow && active_style.shadow > 0 {
                            if active_style.shadow > 1 {
                                this.shadow_md()
                            } else {
                                this.shadow_xs()
                            }
                        } else {
                            this
                        }
                    })
            })
            // M3 TextButton has no container, elevation, or border. Its
            // interaction feedback is only an on-surface-variant state layer.
            .when(
                style == ButtonVariant::Text || style == ButtonVariant::Plain,
                |this| this.border_color(cx.theme().transparent).shadow_none(),
            )
            .when(is_focused && !self.disabled, |this| {
                let focus_paint = button_tokens::resolved_paint(
                    style,
                    button_tokens::ButtonPaintState::Focus,
                    cx,
                );
                let this = this.bg(focus_paint.container);
                if cx.theme().shadow && focus_paint.elevation > 0 {
                    if focus_paint.elevation > 1 {
                        this.shadow_md()
                    } else {
                        this.shadow_xs()
                    }
                } else {
                    this
                }
            })
            .refine_style(&self.style)
            .when_some(terminal_geometry, |this, geometry| {
                this.h(geometry.height)
                    .min_w(geometry.min_width)
                    .pl(geometry.padding_start)
                    .pr(geometry.padding_end)
                    .pt(geometry.padding_top)
                    .pb(geometry.padding_bottom)
                    .rounded_tl(geometry.corners.top_left)
                    .rounded_tr(geometry.corners.top_right)
                    .rounded_bl(geometry.corners.bottom_left)
                    .rounded_br(geometry.corners.bottom_right)
                    .when(style == ButtonVariant::Outlined, |this| {
                        this.when(geometry.border_edges.left, |this| {
                            this.border_l(dimensions.outline)
                        })
                        .when(!geometry.border_edges.left, |this| this.border_l(px(0.)))
                        .when(geometry.border_edges.right, |this| {
                            this.border_r(dimensions.outline)
                        })
                        .when(!geometry.border_edges.right, |this| this.border_r(px(0.)))
                        .when(geometry.border_edges.top, |this| {
                            this.border_t(dimensions.outline)
                        })
                        .when(!geometry.border_edges.top, |this| this.border_t(px(0.)))
                        .when(geometry.border_edges.bottom, |this| {
                            this.border_b(dimensions.outline)
                        })
                        .when(!geometry.border_edges.bottom, |this| this.border_b(px(0.)))
                    })
            })
            .when(self.disabled, |this| {
                let disabled_style = style.disabled(cx);
                this.bg(disabled_style.bg)
                    .text_color(disabled_style.fg)
                    .border_color(disabled_style.border)
                    .shadow_none()
            })
            .when(cursor_disabled, |this| {
                this.cursor(CursorStyle::OperationNotAllowed)
            })
            .on_mouse_down(MouseButton::Left, {
                let ripple_state = ripple_state.clone();
                move |event, window, cx| {
                    // Stop handle any click event when disabled.
                    // To avoid handle dropdown menu open when button is disabled.
                    if is_disabled || !ripple_state.read(cx).is_point_inside(event.position) {
                        cx.stop_propagation();
                        return;
                    }

                    // Avoid focus on mouse down.
                    window.prevent_default();

                    // Pressing a button must not start the window-level text selection.
                    crate::global_state::GlobalState::suppress_text_selection(cx);

                    // Trigger ripple & press hold!
                    crate::ripple::RippleState::start_ripple(
                        ripple_state.clone(),
                        event.position,
                        cx,
                    );
                }
            })
            .on_mouse_up(gpui::MouseButton::Left, {
                let ripple_state = ripple_state.clone();
                move |_, _, cx| {
                    crate::ripple::RippleState::handle_mouse_up(ripple_state.clone(), cx);
                }
            })
            .when_some(self.on_click, |this, on_click| {
                let ripple_state = ripple_state.clone();
                this.on_click(move |event, window, cx| {
                    // Stop handle any click event when disabled or outside rounded curve.
                    if !clickable || !ripple_state.read(cx).is_point_inside(event.position()) {
                        cx.stop_propagation();
                        return;
                    }

                    on_click(event, window, cx);
                })
            })
            .when_some(self.on_hover.filter(|_| hoverable), |this, on_hover| {
                this.on_hover(move |hovered, window, cx| {
                    on_hover(hovered, window, cx);
                })
            })
            .child({
                h_flex()
                    .id("label")
                    .size_full()
                    .items_center()
                    .justify_center()
                    .button_text_size(self.size)
                    .gap(dimensions.gap)
                    .when_some(self.icon, |this, icon| {
                        this.child(
                            icon.id(self.id.clone())
                                .loading_icon(self.loading_icon)
                                .loading(self.loading)
                                .with_size(Size::Size(
                                    self.dropdown_caret_size.unwrap_or(dimensions.icon),
                                )),
                        )
                    })
                    .when_some(self.label, |this, label| {
                        this.child(div().flex_none().line_height(relative(1.)).child(label))
                    })
                    .children(self.children)
                    .when(self.dropdown_caret, |this| {
                        this.justify_between().child(
                            Icon::new(IconName::KeyboardArrowDown)
                                .with_size(Size::Size(
                                    self.dropdown_caret_size.unwrap_or(dimensions.icon),
                                ))
                                .when_some(self.dropdown_caret_rotation, |this, rotation| {
                                    this.rotate(rotation)
                                })
                                .text_color(match self.disabled {
                                    true => normal_style.fg.opacity(0.3),
                                    false => normal_style.fg.opacity(0.5),
                                }),
                        )
                    })
            })
            .when(self.loading && !self.disabled, |this| {
                this.bg(normal_style.bg.opacity(0.8))
                    .border_color(normal_style.border.opacity(0.8))
                    .text_color(normal_style.fg.opacity(0.8))
            })
            .map(|this| {
                if let Some(builder) = self.tooltip_builder {
                    this.managed_tooltip(move |window, cx| builder(window, cx))
                } else if let Some((tooltip, action)) = self.tooltip {
                    this.managed_tooltip(move |window, cx| {
                        Tooltip::new(tooltip.clone())
                            .when_some(action.clone(), |this, (action, context)| {
                                this.action(
                                    action.boxed_clone().as_ref(),
                                    context.as_ref().map(|c| c.as_ref()),
                                )
                            })
                            .build(window, cx)
                    })
                } else {
                    this
                }
            })
            // TextButton focus is represented by the M3 state layer, not the
            // generic primary focus border used by container buttons.
            .when(
                style != ButtonVariant::Text && style != ButtonVariant::Plain,
                |this| this.focus_ring(is_focused, px(0.), window, cx),
            );

        crate::ripple::RippleElement::new(button_element.into_element(), ripple_state)
            .corner_radii(active_radii)
            .color(normal_style.fg)
    }
}

struct ButtonVariantStyle {
    bg: Background,
    border: Hsla,
    fg: Hsla,
    underline: bool,
    shadow: u8,
}

impl ButtonVariant {
    fn normal(&self, cx: &mut App) -> ButtonVariantStyle {
        let paint = button_tokens::resolved_paint(*self, button_tokens::ButtonPaintState::Rest, cx);
        ButtonVariantStyle {
            bg: paint.container.into(),
            border: paint.border,
            fg: paint.content,
            underline: false,
            shadow: paint.elevation,
        }
    }

    fn hovered(&self, cx: &mut App) -> ButtonVariantStyle {
        self.state(cx, button_shared_tokens::STATE_OPACITIES.hover)
    }

    fn pressed(&self, cx: &mut App) -> ButtonVariantStyle {
        self.state(cx, button_shared_tokens::STATE_OPACITIES.pressed)
    }

    fn selected(&self, cx: &mut App) -> ButtonVariantStyle {
        self.normal(cx)
    }

    fn state(&self, cx: &mut App, opacity: f32) -> ButtonVariantStyle {
        let state = if opacity == button_shared_tokens::STATE_HOVER {
            button_tokens::ButtonPaintState::Hover
        } else {
            button_tokens::ButtonPaintState::Pressed
        };
        let paint = button_tokens::resolved_paint(*self, state, cx);
        ButtonVariantStyle {
            bg: paint.container.into(),
            border: paint.border,
            fg: paint.content,
            underline: false,
            shadow: paint.elevation,
        }
    }

    fn disabled(&self, cx: &mut App) -> ButtonVariantStyle {
        let paint =
            button_tokens::resolved_paint(*self, button_tokens::ButtonPaintState::Disabled, cx);
        ButtonVariantStyle {
            bg: paint.container.into(),
            border: paint.border,
            fg: paint.content,
            underline: false,
            shadow: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m3_variants_are_strict() {
        assert_eq!(ButtonVariant::default(), ButtonVariant::Filled);
    }

    #[test]
    fn test_button_full_width_option() {
        let button = Button::new("test-width").full_width(true);
        assert!(button.full_width);
    }
}
