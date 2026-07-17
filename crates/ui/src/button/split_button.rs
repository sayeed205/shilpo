use std::rc::Rc;

use gpui::{
    div, prelude::FluentBuilder, Anchor, App, Context, Edges, ElementId, InteractiveElement as _,
    IntoElement, ParentElement, RenderOnce, SharedString, StyleRefinement, Styled, Window,
    Corners,
};

use crate::{
    menu::{DropdownMenu, PopupMenu},
    tooltip::ComponentTooltip,
    ActiveTheme, Disableable, Sizable, Size, StyledExt as _,
};

use super::{button_dimension_tokens, button_color_tokens, Button, ButtonRounded, ButtonVariant, ButtonVariants};

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
    anchor: Anchor,
    menu: Option<Rc<dyn Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static>>,
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
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let (leading_left, inner_padding, trailing_right) = split_button_paddings(self.size);
        let height = button_dimension_tokens::height(self.size);

        let leading_left = if self.compact { leading_left * 0.5 } else { leading_left };
        let inner_padding = if self.compact { inner_padding * 0.5 } else { inner_padding };
        let trailing_right = if self.compact { trailing_right * 0.5 } else { trailing_right };

        let variant = if self.outline { ButtonVariant::Outlined } else { self.variant };
        let colors = button_color_tokens::colors(variant, cx);

        let leading = self.leading
            .with_variant(variant)
            .with_size(self.size)
            .disabled(self.disabled || self.loading)
            .loading(self.loading)
            .rounded(self.rounded)
            .h(height)
            .border_corners(Corners {
                top_left: true,
                top_right: false,
                bottom_left: true,
                bottom_right: false,
            })
            .border_edges(Edges {
                left: true,
                top: true,
                right: false,
                bottom: true,
            })
            .pl(leading_left)
            .pr(inner_padding);

        let trailing = self.trailing
            .with_variant(variant)
            .with_size(self.size)
            .disabled(self.disabled || self.loading)
            .rounded(self.rounded)
            .h(height)
            .border_corners(Corners {
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
            .pl(inner_padding)
            .pr(trailing_right);

        let divider_color = match variant {
            ButtonVariant::Outlined => colors.border,
            ButtonVariant::Filled | ButtonVariant::FilledTonal | ButtonVariant::Elevated => {
                colors.content.opacity(0.12)
            }
            ButtonVariant::Text => cx.theme().transparent,
        };

        let trailing_element = if let Some(menu) = self.menu {
            let menu = move |pop: PopupMenu, win: &mut Window, ctx: &mut Context<PopupMenu>| -> PopupMenu {
                (menu)(pop, win, ctx)
            };
            trailing.dropdown_menu_with_anchor(self.anchor, menu).into_any_element()
        } else {
            trailing.into_any_element()
        };

        div()
            .id(self.id)
            .h_flex()
            .gap_0()
            .cursor(super::shared::interaction::cursor(
                self.disabled,
                self.loading,
                self.style.mouse_cursor,
            ))
            .refine_style(&self.style)
            .child(leading)
            .child(
                div()
                    .w(gpui::px(1.))
                    .h(height)
                    .bg(divider_color)
            )
            .child(trailing_element)
            .map(|this| self.tooltip.apply(this))
    }
}

fn split_button_paddings(size: Size) -> (gpui::Pixels, gpui::Pixels, gpui::Pixels) {
    match size {
        Size::XSmall => (gpui::px(10.), gpui::px(8.), gpui::px(10.)),
        Size::Small => (gpui::px(16.), gpui::px(12.), gpui::px(16.)),
        Size::Medium => (gpui::px(20.), gpui::px(14.), gpui::px(20.)),
        Size::Large | Size::Size(_) => (gpui::px(32.), gpui::px(24.), gpui::px(32.)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
