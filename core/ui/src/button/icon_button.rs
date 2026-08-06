use std::rc::Rc;

use crate::{
    ActiveTheme, Disableable, Selectable, Sizable, Size, StyledExt, button::ButtonIcon,
    progress::ProgressCircle,
};
use gpui::{
    App, ClickEvent, CursorStyle, Div, ElementId, InteractiveElement, Interactivity, IntoElement,
    ParentElement, RenderOnce, Role, Stateful, StatefulInteractiveElement as _, StyleRefinement,
    Styled, Toggled, Window, div, prelude::FluentBuilder as _,
};

use super::{button_shared_tokens, icon_button_tokens, shared};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IconButtonVariant {
    #[default]
    Standard,
    Filled,
    FilledTonal,
    Outlined,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IconButtonSize {
    XXSmall,
    XSmall,
    Small,
    #[default]
    Medium,
    Large,
    XLarge,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IconButtonWidth {
    #[default]
    Default,
    Narrow,
    Wide,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IconButtonShape {
    #[default]
    Round,
    Square,
}

pub trait IconButtonVariants: Sized {
    fn icon_variant(self, variant: IconButtonVariant) -> Self;
    fn standard(self) -> Self {
        self.icon_variant(IconButtonVariant::Standard)
    }
    fn filled(self) -> Self {
        self.icon_variant(IconButtonVariant::Filled)
    }
    fn filled_tonal(self) -> Self {
        self.icon_variant(IconButtonVariant::FilledTonal)
    }
    fn outlined(self) -> Self {
        self.icon_variant(IconButtonVariant::Outlined)
    }
}

#[derive(IntoElement)]
pub struct IconButton {
    id: ElementId,
    base: Stateful<Div>,
    style: StyleRefinement,
    icon: Option<ButtonIcon>,
    variant: IconButtonVariant,
    size: IconButtonSize,
    shape: IconButtonShape,
    width_type: IconButtonWidth,
    checked: bool,
    checkable: bool,
    disabled: bool,
    loading: bool,
    loading_icon: Option<crate::Icon>,
    full_width: bool,
    on_click: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
    cursor: Option<CursorStyle>,
}

impl IconButton {
    pub fn new(id: impl Into<ElementId>) -> Self {
        let id = id.into();
        Self {
            id: id.clone(),
            base: div().id(id),
            style: StyleRefinement::default(),
            icon: None,
            variant: IconButtonVariant::Standard,
            size: IconButtonSize::Medium,
            shape: IconButtonShape::Round,
            width_type: IconButtonWidth::Default,
            checked: false,
            checkable: false,
            disabled: false,
            loading: false,
            loading_icon: None,
            full_width: false,
            on_click: None,
            cursor: None,
        }
    }

    pub fn icon(mut self, icon: impl Into<ButtonIcon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn size(mut self, size: IconButtonSize) -> Self {
        self.size = size;
        self
    }

    pub fn xxsmall(self) -> Self {
        self.size(IconButtonSize::XXSmall)
    }

    pub fn shape(mut self, shape: IconButtonShape) -> Self {
        self.shape = shape;
        self
    }

    pub fn width(mut self, width: IconButtonWidth) -> Self {
        self.width_type = width;
        self
    }

    pub fn narrow(self) -> Self {
        self.width(IconButtonWidth::Narrow)
    }

    pub fn wide(self) -> Self {
        self.width(IconButtonWidth::Wide)
    }

    pub fn checkable(mut self, checkable: bool) -> Self {
        self.checkable = checkable;
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    pub fn loading_icon(mut self, icon: impl Into<crate::Icon>) -> Self {
        self.loading_icon = Some(icon.into());
        self
    }

    pub fn full_width(mut self, full_width: bool) -> Self {
        self.full_width = full_width;
        self
    }

    pub fn cursor_style(mut self, cursor: CursorStyle) -> Self {
        self.cursor = Some(cursor);
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl IconButtonVariants for IconButton {
    fn icon_variant(mut self, variant: IconButtonVariant) -> Self {
        self.variant = variant;
        self
    }
}

impl Disableable for IconButton {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Selectable for IconButton {
    fn selected(mut self, selected: bool) -> Self {
        self.checked = selected;
        self.checkable = true;
        self
    }

    fn is_selected(&self) -> bool {
        self.checked
    }
}

impl Sizable for IconButton {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = match size.into() {
            Size::XSmall => IconButtonSize::XSmall,
            Size::Small => IconButtonSize::Small,
            Size::Medium => IconButtonSize::Medium,
            Size::Large | Size::Size(_) => IconButtonSize::Large,
        };
        self
    }
}

impl Styled for IconButton {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl InteractiveElement for IconButton {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl RenderOnce for IconButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let dimensions = icon_button_tokens::dimensions(self.size);
        let shapes = icon_button_tokens::shapes(self.size, self.shape);
        let colors = icon_button_tokens::colors(self.variant, self.checked, cx);
        let state = button_shared_tokens::STATE_OPACITIES;
        let focus_handle = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let focused = focus_handle.is_focused(window);

        let ripple_state = window.use_keyed_state(format!("{}-ripple", self.id), cx, |_, _| {
            crate::ripple::RippleState::new()
        });
        let disabled = self.disabled || self.loading;
        let cursor = self.cursor.or(self.style.mouse_cursor);
        let radius = match shapes.shape {
            icon_button_tokens::IconButtonCorner::Full => dimensions.container * 0.5,
            icon_button_tokens::IconButtonCorner::Square(value) => value,
        };
        let pressed_radius = match shapes.pressed_shape {
            icon_button_tokens::IconButtonCorner::Full => dimensions.container * 0.5,
            icon_button_tokens::IconButtonCorner::Square(value) => value,
        };

        let spring_progress = ripple_state.read(cx).current_spring_progress();
        let active_radii = if spring_progress > 0.0 && !disabled {
            crate::motion::lerp_corners(
                gpui::Corners::all(radius),
                gpui::Corners::all(pressed_radius),
                spring_progress,
            )
        } else {
            gpui::Corners::all(radius)
        };

        let width = icon_button_tokens::resolve_width(self.size, self.width_type);

        let icon_button_element = self
            .base
            .role(Role::Button)
            .when(self.checkable, |this| {
                this.aria_toggled(if self.checked {
                    Toggled::True
                } else {
                    Toggled::False
                })
                .aria_selected(self.checked)
            })
            .when(!disabled, |this| this.track_focus(&focus_handle))
            .flex()
            .flex_shrink_0()
            .when(self.full_width, |this| this.w_full())
            .when(!self.full_width, |this| this.w(width))
            .h(dimensions.container)
            .items_center()
            .justify_center()
            .rounded(active_radii.top_left)
            .bg(colors.container)
            .border_color(colors.border)
            .when(self.variant == IconButtonVariant::Outlined, |this| {
                this.border_1()
            })
            .text_color(if disabled {
                cx.theme()
                    .on_surface_variant
                    .opacity(button_shared_tokens::DISABLED_CONTENT_OPACITY)
            } else {
                colors.content
            })
            .when(disabled, |this| {
                this.bg(cx
                    .theme()
                    .on_surface
                    .opacity(button_shared_tokens::DISABLED_CONTAINER_OPACITY))
                    .cursor(shared::interaction::cursor(true, self.loading, cursor))
            })
            .when(!disabled, |this| {
                let hover =
                    shared::interaction::state_layer(colors.container, colors.content, state.hover);
                let pressed = shared::interaction::state_layer(
                    colors.container,
                    colors.content,
                    state.pressed,
                );
                this.cursor(shared::interaction::cursor(false, false, cursor))
                    .hover(|this| this.bg(hover))
                    .active(|this| this.bg(pressed))
            })
            .when(focused && !disabled, |this| {
                this.bg(shared::interaction::state_layer(
                    colors.container,
                    colors.content,
                    state.focus,
                ))
            })
            .on_mouse_down(gpui::MouseButton::Left, {
                let ripple_state = ripple_state.clone();
                move |event, _, cx| {
                    if disabled || !ripple_state.read(cx).is_point_inside(event.position) {
                        cx.stop_propagation();
                        return;
                    }
                    cx.stop_propagation();
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
                    if !disabled {
                        crate::ripple::RippleState::handle_mouse_up(ripple_state.clone(), cx);
                    }
                }
            })
            .when_some(self.on_click.filter(|_| !disabled), |this, on_click| {
                let ripple_state = ripple_state.clone();
                this.on_click(move |event, window, cx| {
                    if !ripple_state.read(cx).is_point_inside(event.position()) {
                        cx.stop_propagation();
                        return;
                    }
                    on_click(event, window, cx);
                })
            })
            .refine_style(&self.style)
            .cursor(shared::interaction::cursor(
                self.disabled,
                self.loading,
                cursor,
            ))
            .when_some(
                self.icon
                    .map(|icon| {
                        icon.id(self.id.clone())
                            .loading(self.loading)
                            .loading_icon(self.loading_icon.clone())
                            .with_size(Size::Size(dimensions.icon))
                    })
                    .or_else(|| {
                        if self.loading {
                            let loading_id = ElementId::Name(format!("{}-loading", self.id).into());
                            Some(
                                ButtonIcon::new(ProgressCircle::new(loading_id).loading(true))
                                    .id(self.id.clone())
                                    .loading(true)
                                    .loading_icon(self.loading_icon.clone())
                                    .with_size(Size::Size(dimensions.icon)),
                            )
                        } else {
                            None
                        }
                    }),
                |this, icon| {
                    this.child(
                        div()
                            .flex()
                            .size(dimensions.icon)
                            .items_center()
                            .justify_center()
                            .child(icon),
                    )
                },
            );

        crate::ripple::RippleElement::new(icon_button_element.into_element(), ripple_state)
            .corner_radii(active_radii)
            .color(colors.content)
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

    struct ClickState {
        count: usize,
    }

    struct ClickRoot {
        state: Entity<ClickState>,
        disabled: bool,
        loading: bool,
    }

    impl Render for ClickRoot {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let state = self.state.clone();
            div().size_full().child(
                div()
                    .debug_selector(|| "icon-button".to_string())
                    .size(px(48.))
                    .child(
                        IconButton::new("icon-button")
                            .icon(IconName::Add)
                            .disabled(self.disabled)
                            .loading(self.loading)
                            .on_click(move |_, _, cx| {
                                state.update(cx, |state, _| state.count += 1);
                            }),
                    ),
            )
        }
    }

    fn click_root(
        cx: &mut TestAppContext,
        disabled: bool,
        loading: bool,
    ) -> (Entity<ClickState>, &mut VisualTestContext) {
        cx.update(crate::init);
        let state = cx.new(|_| ClickState { count: 0 });
        let state_for_root = state.clone();
        let (_, visual) = cx.add_window_view(move |_, _| ClickRoot {
            state: state_for_root,
            disabled,
            loading,
        });
        (state, visual)
    }

    fn draw(visual: &mut VisualTestContext) {
        visual.run_until_parked();
        visual.update(|window, cx| {
            _ = window.draw(cx);
        });
    }

    #[test]
    fn test_icon_button_square_radius() {
        let shapes = icon_button_tokens::shapes(IconButtonSize::Medium, IconButtonShape::Square);
        let radius = match shapes.shape {
            icon_button_tokens::IconButtonCorner::Full => px(24.),
            icon_button_tokens::IconButtonCorner::Square(value) => value,
        };
        assert_eq!(radius, px(16.));
    }

    #[test]
    fn variants_and_toggle_state_are_distinct() {
        assert_eq!(IconButtonVariant::default(), IconButtonVariant::Standard);
        let button = IconButton::new("toggle").checkable(true).checked(true);
        assert!(button.checkable);
        assert!(button.checked);
    }

    #[test]
    fn icon_button_size_table_is_static() {
        let expected = [
            (IconButtonSize::XSmall, 32., 16.),
            (IconButtonSize::Small, 40., 20.),
            (IconButtonSize::Medium, 48., 24.),
            (IconButtonSize::Large, 56., 28.),
            (IconButtonSize::XLarge, 72., 32.),
        ];
        for (size, container, icon) in expected {
            let dimensions = icon_button_tokens::dimensions(size);
            assert_eq!(dimensions.container, px(container));
            assert_eq!(dimensions.icon, px(icon));
        }
    }

    #[test]
    fn icon_button_shape_table_is_static() {
        let expected = [
            (IconButtonSize::XSmall, 12.),
            (IconButtonSize::Small, 12.),
            (IconButtonSize::Medium, 16.),
            (IconButtonSize::Large, 28.),
            (IconButtonSize::XLarge, 28.),
        ];
        for (size, radius) in expected {
            let shapes = icon_button_tokens::shapes(size, IconButtonShape::Square);
            assert_eq!(
                shapes.shape,
                icon_button_tokens::IconButtonCorner::Square(px(radius))
            );
            assert!(matches!(
                shapes.pressed_shape,
                icon_button_tokens::IconButtonCorner::Square(_)
            ));
        }

        for size in [
            IconButtonSize::XSmall,
            IconButtonSize::Small,
            IconButtonSize::Medium,
            IconButtonSize::Large,
            IconButtonSize::XLarge,
        ] {
            let shapes = icon_button_tokens::shapes(size, IconButtonShape::Round);
            assert_eq!(shapes.shape, icon_button_tokens::IconButtonCorner::Full);
            assert_eq!(
                shapes.pressed_shape,
                icon_button_tokens::IconButtonCorner::Full
            );
        }
    }

    #[gpui::test]
    fn rendered_enabled_icon_button_click_mutates_entity_once(cx: &mut TestAppContext) {
        let (state, visual) = click_root(cx, false, false);
        draw(visual);
        let bounds = visual
            .debug_bounds("icon-button")
            .expect("icon button bounds");
        visual.simulate_mouse_move(bounds.center(), None, Default::default());
        visual.simulate_click(bounds.center(), Default::default());
        assert_eq!(state.read_with(visual, |state, _| state.count), 1);
    }

    #[gpui::test]
    fn rendered_disabled_and_loading_icon_buttons_do_not_click(cx: &mut TestAppContext) {
        for (disabled, loading) in [(true, false), (false, true)] {
            let (state, visual) = click_root(cx, disabled, loading);
            draw(visual);
            let bounds = visual
                .debug_bounds("icon-button")
                .expect("icon button bounds");
            visual.simulate_click(bounds.center(), Default::default());
            assert_eq!(state.read_with(visual, |state, _| state.count), 0);
        }
    }

    #[test]
    fn test_icon_button_loading_and_full_width_options() {
        let button = IconButton::new("test-options")
            .loading(true)
            .full_width(true);
        assert!(button.loading);
        assert!(button.full_width);
    }
}
