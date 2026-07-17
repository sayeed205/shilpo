use std::rc::Rc;

use crate::{button::ButtonIcon, ActiveTheme, Disableable, Selectable, Sizable, Size, StyledExt};
use gpui::{
    div, prelude::FluentBuilder as _, App, ClickEvent, CursorStyle, Div, ElementId,
    InteractiveElement, Interactivity, IntoElement, ParentElement, RenderOnce, Role, Stateful,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Toggled, Window,
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
        let disabled = self.disabled || self.loading;
        let cursor = self.cursor.or(self.style.mouse_cursor);
        let radius = match shapes.shape {
            icon_button_tokens::IconButtonCorner::Full => dimensions.container * 0.5,
            icon_button_tokens::IconButtonCorner::Square(value) => value,
        };

        let width = icon_button_tokens::resolve_width(self.size, self.width_type);

        self.base
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
            .w(width)
            .h(dimensions.container)
            .items_center()
            .justify_center()
            .rounded(radius)
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
            .when_some(self.on_click.filter(|_| !disabled), |this, on_click| {
                this.on_click(move |event, window, cx| on_click(event, window, cx))
            })
            .refine_style(&self.style)
            .cursor(shared::interaction::cursor(
                self.disabled,
                self.loading,
                cursor,
            ))
            .when_some(
                self.icon
                    .map(|icon| icon.with_size(Size::Size(dimensions.icon))),
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
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IconName;
    use gpui::{
        div, px, AppContext, Context, Entity, IntoElement, Render, TestAppContext,
        VisualTestContext, Window,
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
                            .icon(IconName::Plus)
                            .disabled(self.disabled)
                            .loading(self.loading)
                            .on_click(move |_, _, cx| {
                                state.update(cx, |state, _| state.count += 1);
                            }),
                    ),
            )
        }
    }

    fn click_root<'a>(
        cx: &'a mut TestAppContext,
        disabled: bool,
        loading: bool,
    ) -> (Entity<ClickState>, &'a mut VisualTestContext) {
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
        assert_eq!(radius, px(12.));
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
        assert_eq!(
            icon_button_tokens::dimensions(IconButtonSize::XSmall).container,
            px(32.)
        );
        assert_eq!(
            icon_button_tokens::dimensions(IconButtonSize::Medium).icon,
            px(24.)
        );
        assert_eq!(
            icon_button_tokens::dimensions(IconButtonSize::XLarge).container,
            px(72.)
        );
    }

    #[gpui::test]
    fn rendered_enabled_icon_button_click_mutates_entity_once(cx: &mut TestAppContext) {
        let (state, mut visual) = click_root(cx, false, false);
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
            let (state, mut visual) = click_root(cx, disabled, loading);
            draw(visual);
            let bounds = visual
                .debug_bounds("icon-button")
                .expect("icon button bounds");
            visual.simulate_click(bounds.center(), Default::default());
            assert_eq!(state.read_with(visual, |state, _| state.count), 0);
        }
    }
}
