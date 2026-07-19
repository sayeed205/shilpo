use std::rc::Rc;

use gpui::{
    Anchor, App, Context, Corners, ElementId, InteractiveElement as _, IntoElement,
    ParentElement, RenderOnce, SharedString, StyleRefinement, Styled, Window, div,
    prelude::FluentBuilder,
};

use crate::{
    Disableable, Sizable, Size, StyledExt as _,
    menu::{DropdownMenu, PopupMenu},
    tooltip::ComponentTooltip,
};

use super::{
    Button, ButtonRounded, ButtonVariant, ButtonVariants, SplitButtonShapes,
    split_button_tokens,
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
        let outer_radius = crate::button::button_shape_tokens::resolve(
            self.rounded,
            self.size,
            Some(height),
        );
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

        let trailing = self
            .trailing
            .with_variant(variant)
            .with_size(self.size)
            .disabled(self.disabled || self.loading)
            .loading(self.loading)
            .h(height)
            .corner_radii(trailing_corners)
            .border_corners(Corners {
                top_left: true,
                top_right: true,
                bottom_left: true,
                bottom_right: true,
            })
            .pl(trailing_inner)
            .pr(trailing_right)
            .min_w(tokens.min_width);

        let trailing_element = if let Some(menu) = self.menu {
            let menu = move |pop: PopupMenu,
                             win: &mut Window,
                             ctx: &mut Context<PopupMenu>|
                  -> PopupMenu { (menu)(pop, win, ctx) };
            trailing
                .dropdown_menu_with_anchor(self.anchor, menu)
                .into_any_element()
        } else {
            trailing.into_any_element()
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
}
