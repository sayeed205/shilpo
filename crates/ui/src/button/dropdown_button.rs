use gpui::{
    div, prelude::FluentBuilder, Anchor, App, Context, ElementId, InteractiveElement as _,
    IntoElement, ParentElement, RenderOnce, SharedString, StyleRefinement, Styled, Window,
};
use gpui::CursorStyle;

use crate::{
    menu::{DropdownMenu, PopupMenu},
    tooltip::ComponentTooltip,
    Disableable, Selectable, Sizable, Size, StyledExt as _,
};

use super::{shared, Button, ButtonRounded, ButtonVariant, ButtonVariants, button_dimension_tokens};

#[derive(IntoElement)]
pub struct DropdownButton {
    id: ElementId,
    style: StyleRefinement,
    button: Option<Button>,
    menu:
        Option<Box<dyn Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static>>,
    selected: bool,
    disabled: bool,
    // The button props
    compact: bool,
    outline: bool,
    loading: bool,
    variant: ButtonVariant,
    size: Size,
    rounded: ButtonRounded,
    anchor: Anchor,
    tooltip: ComponentTooltip,
}

impl DropdownButton {
    /// Create a new DropdownButton.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            button: None,
            menu: None,
            selected: false,
            disabled: false,
            compact: false,
            outline: false,
            loading: false,
            variant: ButtonVariant::default(),
            size: Size::default(),
            rounded: ButtonRounded::default(),
            anchor: Anchor::TopRight,
            tooltip: ComponentTooltip::default(),
        }
    }

    /// Override default pointing-hand cursor for this DropdownButton.
    ///
    /// Applied after DropdownButton defaults, so caller choice wins.
    pub fn cursor(mut self, cursor: CursorStyle) -> Self {
        self.style.mouse_cursor = Some(cursor);
        self
    }

    /// Set tooltip text for the dropdown button.
    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip.text = Some((tooltip.into(), None));
        self
    }

    /// Set the left button of the dropdown button.
    pub fn button(mut self, button: Button) -> Self {
        self.button = Some(button);
        self
    }

    /// Set the dropdown menu of the button.
    pub fn dropdown_menu(
        mut self,
        menu: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> Self {
        self.menu = Some(Box::new(menu));
        self
    }

    /// Set the dropdown menu of the button with anchor corner.
    pub fn dropdown_menu_with_anchor(
        mut self,
        anchor: impl Into<Anchor>,
        menu: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> Self {
        self.menu = Some(Box::new(menu));
        self.anchor = anchor.into();
        self
    }

    /// Set the rounded style of the button.
    pub fn rounded(mut self, rounded: impl Into<ButtonRounded>) -> Self {
        self.rounded = rounded.into();
        self
    }

    /// Set the button to compact style.
    ///
    /// See also: [`Button::compact`]
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    /// Set the button to outline style.
    ///
    /// See also: [`Button::outline`]
    pub fn outline(mut self) -> Self {
        self.outline = true;
        self
    }

    /// Set the button to loading state.
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }
}

impl Disableable for DropdownButton {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Styled for DropdownButton {
    fn style(&mut self) -> &mut gpui::StyleRefinement {
        &mut self.style
    }
}

impl Sizable for DropdownButton {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl ButtonVariants for DropdownButton {
    fn with_variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }
}

impl Selectable for DropdownButton {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl RenderOnce for DropdownButton {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let cursor = self.style.mouse_cursor;
        let height = button_dimension_tokens::height(self.size);
        let button = self.button.unwrap_or_else(|| Button::new(self.id.clone()));

        let trigger = button
            .rounded(self.rounded)
            .h(height)
            .loading(self.loading)
            .selected(self.selected)
            .disabled(self.disabled || self.loading)
            .dropdown_caret(true)
            .when(self.compact, |this| this.compact())
            .when(self.outline, |this| this.outline())
            .when(self.size != Size::Medium, |this| this.with_size(self.size))
            .with_variant(self.variant);

        let element = if let Some(menu) = self.menu {
            trigger.dropdown_menu_with_anchor(self.anchor, menu).into_any_element()
        } else {
            trigger.into_any_element()
        };

        div()
            .id(self.id)
            .cursor(shared::interaction::cursor(
                self.disabled,
                self.loading,
                cursor,
            ))
            .refine_style(&self.style)
            .child(element)
            .map(|this| self.tooltip.apply(this))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn test_dropdown_button_builder(_cx: &mut gpui::TestAppContext) {
        let button = Button::new("inner").label("Action");
        let dropdown = DropdownButton::new("complex-dropdown")
            .button(button)
            .filled()
            .outline()
            .large()
            .compact()
            .loading(false)
            .disabled(false)
            .selected(false)
            .rounded(ButtonRounded::Medium)
            .dropdown_menu_with_anchor(Anchor::BottomLeft, |menu, _, _| menu);

        assert!(dropdown.button.is_some());
        assert_eq!(dropdown.variant, ButtonVariant::Filled);
        assert!(dropdown.outline);
        assert_eq!(dropdown.size, Size::Large);
        assert!(dropdown.compact);
        assert!(!dropdown.loading);
        assert!(!dropdown.disabled);
        assert!(!dropdown.selected);
        assert!(matches!(dropdown.rounded, ButtonRounded::Medium));
        assert!(dropdown.menu.is_some());
        assert_eq!(dropdown.anchor, Anchor::BottomLeft);
    }
}
