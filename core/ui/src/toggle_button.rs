use std::rc::Rc;

use gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled, Window,
    div, prelude::FluentBuilder as _, px,
};

use crate::{ActiveTheme, Colorize, Disableable, Icon, IconName, Sizable, Size};

/// Material 3 Expressive ToggleButton variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ToggleButtonVariant {
    /// Filled container toggle button.
    #[default]
    Filled,
    /// Elevated shadow container toggle button.
    Elevated,
    /// Outlined container toggle button.
    Outlined,
    /// Tonal container toggle button.
    Tonal,
}

/// A Material Design 3 Expressive ToggleButton component.
///
/// Toggle buttons allow users to toggle an option on or off.
///
/// # Reference
/// AndroidX `ToggleButton.kt` — `ToggleButton`, `ElevatedToggleButton`, `OutlinedToggleButton`, `TonalToggleButton`.
#[derive(IntoElement)]
pub struct ToggleButton {
    id: ElementId,
    variant: ToggleButtonVariant,
    label: Option<SharedString>,
    icon: Option<IconName>,
    checked: bool,
    disabled: bool,
    size: Size,
    on_change: Option<Rc<dyn Fn(bool, &ClickEvent, &mut Window, &mut App)>>,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl ToggleButton {
    /// Creates a new ToggleButton.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            variant: ToggleButtonVariant::Filled,
            label: None,
            icon: None,
            checked: false,
            disabled: false,
            size: Size::Medium,
            on_change: None,
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// Creates a filled ToggleButton.
    pub fn filled(id: impl Into<ElementId>) -> Self {
        Self::new(id).variant(ToggleButtonVariant::Filled)
    }

    /// Creates an elevated ToggleButton.
    pub fn elevated(id: impl Into<ElementId>) -> Self {
        Self::new(id).variant(ToggleButtonVariant::Elevated)
    }

    /// Creates an outlined ToggleButton.
    pub fn outlined(id: impl Into<ElementId>) -> Self {
        Self::new(id).variant(ToggleButtonVariant::Outlined)
    }

    /// Creates a tonal ToggleButton.
    pub fn tonal(id: impl Into<ElementId>) -> Self {
        Self::new(id).variant(ToggleButtonVariant::Tonal)
    }

    /// Sets the variant (Filled, Elevated, Outlined, Tonal).
    pub fn variant(mut self, variant: ToggleButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Sets the checked state.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// Sets label text.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Sets icon name.
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Sets checked change callback.
    pub fn on_change(
        mut self,
        handler: impl Fn(bool, &ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }

    /// Appends a custom child element.
    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }
}

impl Styled for ToggleButton {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Sizable for ToggleButton {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl Disableable for ToggleButton {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for ToggleButton {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let checked = self.checked;
        let is_disabled = self.disabled;

        let (height, font_size, icon_size, padding_x) = match self.size {
            Size::XSmall => (px(28.), px(11.), px(14.), px(10.)),
            Size::Small => (px(32.), px(12.), px(16.), px(12.)),
            Size::Medium => (px(40.), px(13.), px(18.), px(16.)),
            Size::Large => (px(48.), px(15.), px(20.), px(20.)),
            Size::Size(s) => (s, px(13.), px(18.), px(16.)),
        };

        let (bg, border_color, fg, shadow) = if is_disabled {
            (
                cx.theme().on_surface.opacity(0.12),
                cx.theme().outline.opacity(0.12),
                cx.theme().on_surface.opacity(0.38),
                false,
            )
        } else if checked {
            match self.variant {
                ToggleButtonVariant::Filled | ToggleButtonVariant::Elevated => (
                    cx.theme().primary,
                    cx.theme().primary,
                    cx.theme().on_primary,
                    self.variant == ToggleButtonVariant::Elevated,
                ),
                ToggleButtonVariant::Outlined | ToggleButtonVariant::Tonal => (
                    cx.theme().secondary_container,
                    cx.theme().secondary_container,
                    cx.theme().on_secondary_container,
                    false,
                ),
            }
        } else {
            match self.variant {
                ToggleButtonVariant::Filled => (
                    cx.theme().surface_container_highest,
                    cx.theme().surface_container_highest,
                    cx.theme().on_surface_variant,
                    false,
                ),
                ToggleButtonVariant::Elevated => (
                    cx.theme().surface_container_low,
                    cx.theme().surface_container_low,
                    cx.theme().on_surface,
                    true,
                ),
                ToggleButtonVariant::Outlined => (
                    cx.theme().surface,
                    cx.theme().outline,
                    cx.theme().on_surface_variant,
                    false,
                ),
                ToggleButtonVariant::Tonal => (
                    cx.theme().surface_container,
                    cx.theme().surface_container,
                    cx.theme().on_surface_variant,
                    false,
                ),
            }
        };

        let hover_bg = if !is_disabled {
            Some(bg.darken(0.06))
        } else {
            None
        };

        let on_change_fn = self.on_change.clone();

        div()
            .id(self.id)
            .h(height)
            .px(padding_x)
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .gap_2()
            .bg(bg)
            .border_1()
            .border_color(border_color)
            .text_size(font_size)
            .text_color(fg)
            .when(shadow, |this| this.shadow_sm())
            .when(!is_disabled, |this| {
                this.cursor_pointer()
                    .when_some(hover_bg, |this, bg| this.hover(|s| s.bg(bg)))
            })
            .when_some(on_change_fn.filter(|_| !is_disabled), |this, handler| {
                this.on_click(move |evt, window, cx| {
                    handler(!checked, evt, window, cx);
                })
            })
            .when_some(self.icon, |this, icon_name| {
                this.child(Icon::new(icon_name).size(icon_size).text_color(fg))
            })
            .when_some(self.label, |this, text| this.child(text))
            .children(self.children)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toggle_button_variants() {
        let filled = ToggleButton::filled("tb-1");
        assert_eq!(filled.variant, ToggleButtonVariant::Filled);

        let elevated = ToggleButton::elevated("tb-2");
        assert_eq!(elevated.variant, ToggleButtonVariant::Elevated);

        let outlined = ToggleButton::outlined("tb-3");
        assert_eq!(outlined.variant, ToggleButtonVariant::Outlined);

        let tonal = ToggleButton::tonal("tb-4");
        assert_eq!(tonal.variant, ToggleButtonVariant::Tonal);
    }
}
