use std::rc::Rc;

use crate::{
    button::ButtonIcon,
    h_flex,
    tooltip::{ManagedTooltipExt as _, Tooltip},
    ActiveTheme, Disableable, FocusableExt as _, Icon, IconName, Selectable, Sizable, Size,
    StyleSized, StyledExt,
};
use gpui::{
    div, prelude::FluentBuilder as _, px, relative, AnyElement, App, Background, ClickEvent,
    Corners, CursorStyle, Div, Edges, ElementId, Hsla, InteractiveElement, Interactivity,
    IntoElement, MouseButton, ParentElement, Pixels, RenderOnce, Role, SharedString, Stateful,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window,
};

use super::{
    button_dimension_tokens, button_shape_tokens, button_shared_tokens,
    button_tokens::{self, ButtonElevation},
    shared,
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
    loading_icon: Option<Icon>,
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
            border_corners: Corners {
                top_left: true,
                top_right: true,
                bottom_right: true,
                bottom_left: true,
            },
            border_edges: Edges::all(true),
            size: Size::Medium,
            tooltip: None,
            tooltip_builder: None,
            on_click: None,
            on_hover: None,
            loading: false,
            compact: false,
            outline: false,
            children: Vec::new(),
            loading_icon: None,
            dropdown_caret: false,
            pl: None,
            pr: None,
            tab_index: 0,
            tab_stop: true,
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

    /// Set the border edges of the Button.
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
        let clickable = self.clickable();
        let is_disabled = self.disabled;
        let cursor_disabled = self.disabled || self.loading;
        let hoverable = self.hoverable();
        let normal_style = style.normal(cx);
        let state_tokens = button_shared_tokens::STATE_OPACITIES;
        let color_tokens = button_tokens::tokens(style, cx);
        let icon_only = self.label.is_none() && self.children.is_empty() && self.icon.is_some();
        let dimensions = button_dimension_tokens::resolve(
            self.size,
            style == ButtonVariant::Text,
            self.compact,
            icon_only,
        );
        let icon_size = match self.size {
            Size::Size(v) => Size::Size(v * 0.75),
            _ => self.size,
        };

        let focus_handle = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let is_focused = focus_handle.is_focused(window);

        let rounding =
            button_shape_tokens::resolve(self.rounded, self.size, Some(dimensions.height));

        self.base
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
            .h(dimensions.height)
            .min_w(dimensions.min_width)
            .when_some(self.pl, |this, pl| this.pl(pl))
            .when_some(self.pr, |this, pr| this.pr(pr))
            .when(self.pl.is_none() && self.pr.is_none(), |this| {
                this.px(dimensions.horizontal_padding)
            })
            .py(dimensions.vertical_padding)
            .when(cx.theme().shadow && normal_style.shadow, |this| {
                this.shadow_xs()
            })
            .when(self.border_corners.top_left, |this| {
                this.rounded_tl(rounding)
            })
            .when(self.border_corners.top_right, |this| {
                this.rounded_tr(rounding)
            })
            .when(self.border_corners.bottom_left, |this| {
                this.rounded_bl(rounding)
            })
            .when(self.border_corners.bottom_right, |this| {
                this.rounded_br(rounding)
            })
            .when(style == ButtonVariant::Outlined, |this| {
                this.when(self.border_edges.left, |this| this.border_l_1())
                    .when(self.border_edges.right, |this| this.border_r_1())
                    .when(self.border_edges.top, |this| this.border_t_1())
                    .when(self.border_edges.bottom, |this| this.border_b_1())
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
                        this.bg(hover_style.bg)
                            .border_color(hover_style.border)
                            .text_color(hover_style.fg)
                    })
                    .active(|this| {
                        let active_style = style.pressed(cx);
                        this.bg(active_style.bg)
                            .border_color(active_style.border)
                            .text_color(active_style.fg)
                    })
            })
            .when(self.disabled, |this| {
                let disabled_style = style.disabled(cx);
                this.bg(disabled_style.bg)
                    .text_color(disabled_style.fg)
                    .border_color(disabled_style.border)
                    .shadow_none()
            })
            // M3 TextButton has no container, elevation, or border. Its
            // interaction feedback is only an on-surface-variant state layer.
            .when(style == ButtonVariant::Text, |this| {
                this.border_color(cx.theme().transparent).shadow_none()
            })
            .when(is_focused && !self.disabled, |this| {
                this.bg(shared::interaction::state_layer(
                    color_tokens.container,
                    color_tokens.content,
                    state_tokens.focus,
                ))
            })
            .refine_style(&self.style)
            .when(cursor_disabled, |this| {
                this.cursor(CursorStyle::OperationNotAllowed)
            })
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                // Stop handle any click event when disabled.
                // To avoid handle dropdown menu open when button is disabled.
                if is_disabled {
                    cx.stop_propagation();
                    return;
                }

                // Avoid focus on mouse down.
                window.prevent_default();

                // Pressing a button must not start the window-level text selection.
                crate::global_state::GlobalState::suppress_text_selection(cx);
            })
            .when_some(self.on_click, |this, on_click| {
                this.on_click(move |event, window, cx| {
                    // Stop handle any click event when disabled.
                    // To avoid handle dropdown menu open when button is disabled.
                    if !clickable {
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
                    .map(|this| match self.size {
                        Size::XSmall => this.gap_1(),
                        Size::Small => this.gap_1(),
                        _ => this.gap_2(),
                    })
                    .when_some(self.icon, |this, icon| {
                        this.child(
                            icon.loading_icon(self.loading_icon)
                                .loading(self.loading)
                                .with_size(icon_size),
                        )
                    })
                    .when_some(self.label, |this, label| {
                        this.child(div().flex_none().line_height(relative(1.)).child(label))
                    })
                    .children(self.children)
                    .when(self.dropdown_caret, |this| {
                        this.justify_between().child(
                            Icon::new(IconName::ChevronDown).xsmall().text_color(
                                match self.disabled {
                                    true => normal_style.fg.opacity(0.3),
                                    false => normal_style.fg.opacity(0.5),
                                },
                            ),
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
            .when(style != ButtonVariant::Text, |this| {
                this.focus_ring(is_focused, px(0.), window, cx)
            })
    }
}

struct ButtonVariantStyle {
    bg: Background,
    border: Hsla,
    fg: Hsla,
    underline: bool,
    shadow: bool,
}

impl ButtonVariant {
    fn normal(&self, cx: &mut App) -> ButtonVariantStyle {
        let tokens = button_tokens::tokens(*self, cx);
        ButtonVariantStyle {
            bg: tokens.container.into(),
            border: tokens.border,
            fg: tokens.content,
            underline: false,
            shadow: matches!(tokens.elevation, ButtonElevation::Level1),
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
        let tokens = button_tokens::tokens(*self, cx);
        ButtonVariantStyle {
            bg: shared::interaction::state_layer(tokens.container, tokens.content, opacity),
            border: tokens.border,
            fg: tokens.content,
            underline: false,
            shadow: matches!(tokens.elevation, ButtonElevation::Level1),
        }
    }

    fn disabled(&self, cx: &mut App) -> ButtonVariantStyle {
        let container = match self {
            ButtonVariant::FilledTonal => cx
                .theme()
                .on_surface
                .opacity(button_shared_tokens::DISABLED_CONTAINER_OPACITY),
            _ => cx
                .theme()
                .on_surface
                .opacity(button_shared_tokens::DISABLED_CONTAINER_OPACITY),
        };
        ButtonVariantStyle {
            bg: container.into(),
            border: if matches!(self, ButtonVariant::Outlined) {
                cx.theme().outline_variant
            } else {
                cx.theme().transparent
            },
            fg: cx
                .theme()
                .on_surface_variant
                .opacity(button_shared_tokens::DISABLED_CONTENT_OPACITY),
            underline: false,
            shadow: false,
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
}
