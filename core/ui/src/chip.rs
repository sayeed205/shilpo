use std::rc::Rc;

use gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled, Window,
    div, prelude::FluentBuilder as _, px,
};

use crate::{ActiveTheme, Colorize, Disableable, Icon, IconName, Sizable, Size, StyledExt};

/// The Material 3 Expressive chip variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ChipVariant {
    /// Contextual action trigger chip.
    #[default]
    Assist,
    /// Toggleable selection chip with selected state and checkmark.
    Filter,
    /// Information token chip with avatar/icon slot and trailing close button.
    Input,
    /// Simple suggestion prompt chip.
    Suggestion,
}

/// Visual elevation style of the chip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ChipStyle {
    /// Flat container with border.
    #[default]
    Flat,
    /// Elevated container with shadow.
    Elevated,
}

/// A Material Design 3 Expressive Chip component.
///
/// Chips allow users to enter information, make selections, filter content,
/// or trigger contextual actions.
///
/// # Reference
/// AndroidX `Chip.kt` — `AssistChip`, `FilterChip`, `InputChip`, `SuggestionChip`.
#[derive(IntoElement)]
pub struct Chip {
    id: ElementId,
    variant: ChipVariant,
    chip_style: ChipStyle,
    label: SharedString,
    size: Size,
    selected: bool,
    disabled: bool,
    leading_icon: Option<AnyElement>,
    trailing_icon: Option<AnyElement>,
    on_click: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
    on_dismiss: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
    style: StyleRefinement,
}

impl Chip {
    /// Creates a new AssistChip with the specified ID and label.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            variant: ChipVariant::Assist,
            chip_style: ChipStyle::Flat,
            label: label.into(),
            size: Size::Medium,
            selected: false,
            disabled: false,
            leading_icon: None,
            trailing_icon: None,
            on_click: None,
            on_dismiss: None,
            style: StyleRefinement::default(),
        }
    }

    /// Creates an AssistChip.
    pub fn assist(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self::new(id, label).variant(ChipVariant::Assist)
    }

    /// Creates a FilterChip.
    pub fn filter(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self::new(id, label).variant(ChipVariant::Filter)
    }

    /// Creates an InputChip.
    pub fn input(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self::new(id, label).variant(ChipVariant::Input)
    }

    /// Creates a SuggestionChip.
    pub fn suggestion(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self::new(id, label).variant(ChipVariant::Suggestion)
    }

    /// Sets the chip variant (Assist, Filter, Input, Suggestion).
    pub fn variant(mut self, variant: ChipVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Sets whether the chip uses an elevated visual container style.
    pub fn elevated(mut self, elevated: bool) -> Self {
        self.chip_style = if elevated {
            ChipStyle::Elevated
        } else {
            ChipStyle::Flat
        };
        self
    }

    /// Sets the selection state (primarily for FilterChip).
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Sets a leading icon or avatar element.
    pub fn leading_icon(mut self, icon: impl IntoElement) -> Self {
        self.leading_icon = Some(icon.into_any_element());
        self
    }

    /// Sets a trailing icon element.
    pub fn trailing_icon(mut self, icon: impl IntoElement) -> Self {
        self.trailing_icon = Some(icon.into_any_element());
        self
    }

    /// Sets a click event handler for the chip.
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }

    /// Sets a dismiss handler for the trailing close × button (for InputChip).
    pub fn on_dismiss(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_dismiss = Some(Rc::new(handler));
        self
    }
}

impl Styled for Chip {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Sizable for Chip {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl Disableable for Chip {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for Chip {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let is_disabled = self.disabled;
        let selected = self.selected;
        let variant = self.variant;
        let elevated = self.chip_style == ChipStyle::Elevated;

        let (height, padding_x, font_size, icon_size) = match self.size {
            Size::XSmall => (px(24.), px(8.), px(11.), px(14.)),
            Size::Small => (px(28.), px(10.), px(12.), px(16.)),
            Size::Medium => (px(32.), px(12.), px(13.), px(18.)),
            Size::Large => (px(38.), px(16.), px(14.), px(20.)),
            Size::Size(s) => (s, px(12.), px(13.), px(18.)),
        };

        // Determine background and text colors based on state & variant
        let (bg, border_color, fg) = if is_disabled {
            (
                cx.theme().on_surface.opacity(0.12),
                cx.theme().outline.opacity(0.12),
                cx.theme().on_surface.opacity(0.38),
            )
        } else if selected {
            (
                cx.theme().secondary_container,
                cx.theme().secondary_container,
                cx.theme().on_secondary_container,
            )
        } else if elevated {
            (
                cx.theme().surface_container_low,
                cx.theme().surface_container_low,
                cx.theme().on_surface,
            )
        } else {
            (
                cx.theme().surface,
                cx.theme().outline.opacity(0.5),
                cx.theme().on_surface_variant,
            )
        };

        let hover_bg = if selected {
            cx.theme().secondary_container.darken(0.08)
        } else {
            cx.theme().on_surface.opacity(0.08)
        };

        // Automatic leading checkmark icon for selected FilterChip
        let leading = if variant == ChipVariant::Filter && selected {
            Some(
                Icon::new(IconName::Check)
                    .size(icon_size)
                    .text_color(fg)
                    .into_any_element(),
            )
        } else {
            self.leading_icon
        };

        // Automatic trailing close icon for InputChip if on_dismiss is set
        let on_dismiss_fn = self.on_dismiss.clone();
        let trailing = if variant == ChipVariant::Input && on_dismiss_fn.is_some() {
            Some(
                div()
                    .id("chip-dismiss")
                    .cursor_pointer()
                    .child(Icon::new(IconName::Close).size(icon_size).text_color(fg))
                    .when_some(on_dismiss_fn, |this, handler| {
                        this.on_click(move |evt, window, cx| {
                            handler(evt, window, cx);
                        })
                    })
                    .into_any_element(),
            )
        } else {
            self.trailing_icon
        };

        div()
            .id(self.id)
            .h(height)
            .px(padding_x)
            .rounded_full()
            .flex()
            .items_center()
            .gap_1_5()
            .bg(bg)
            .border_1()
            .border_color(border_color)
            .text_size(font_size)
            .text_color(fg)
            .font_medium()
            .when(elevated && !is_disabled, |this| this.shadow_sm())
            .when(!is_disabled, |this| {
                this.cursor_pointer().hover(|style| style.bg(hover_bg))
            })
            .when_some(self.on_click.filter(|_| !is_disabled), |this, handler| {
                this.on_click(move |evt, window, cx| {
                    handler(evt, window, cx);
                })
            })
            .when_some(leading, |this, icon| this.child(icon))
            .child(self.label)
            .when_some(trailing, |this, icon| this.child(icon))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chip_variants() {
        let assist = Chip::assist("c1", "Assist");
        assert_eq!(assist.variant, ChipVariant::Assist);

        let filter = Chip::filter("c2", "Filter").selected(true);
        assert_eq!(filter.variant, ChipVariant::Filter);
        assert!(filter.selected);

        let input = Chip::input("c3", "Input");
        assert_eq!(input.variant, ChipVariant::Input);

        let suggestion = Chip::suggestion("c4", "Suggestion");
        assert_eq!(suggestion.variant, ChipVariant::Suggestion);
    }
}
