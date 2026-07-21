use gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled, Window,
    div, prelude::FluentBuilder as _,
};
use std::rc::Rc;

use crate::{ActiveTheme, Colorize, Disableable, StyledExt, v_flex};

/// Material 3 Expressive Card variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CardVariant {
    /// Filled container with subtle background contrast.
    #[default]
    Filled,
    /// Elevated container with drop shadow.
    Elevated,
    /// Outlined container with border stroke.
    Outlined,
}

/// A Material Design 3 Expressive Card container component.
///
/// Cards display content and actions about a single subject.
///
/// # Reference
/// AndroidX `Card.kt` — `Card`, `ElevatedCard`, `OutlinedCard`.
#[derive(IntoElement)]
pub struct Card {
    id: Option<ElementId>,
    variant: CardVariant,
    disabled: bool,
    on_click: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl Card {
    /// Creates a new filled Card.
    pub fn new() -> Self {
        Self {
            id: None,
            variant: CardVariant::Filled,
            disabled: false,
            on_click: None,
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// Sets the element ID (required if card is interactive).
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Creates a filled Card.
    pub fn filled() -> Self {
        Self::new().variant(CardVariant::Filled)
    }

    /// Creates an elevated Card.
    pub fn elevated() -> Self {
        Self::new().variant(CardVariant::Elevated)
    }

    /// Creates an outlined Card.
    pub fn outlined() -> Self {
        Self::new().variant(CardVariant::Outlined)
    }

    /// Sets the card variant (Filled, Elevated, Outlined).
    pub fn variant(mut self, variant: CardVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Sets a click handler on the card.
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }

    /// Appends a single child element.
    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }

    /// Appends multiple child elements.
    pub fn children(mut self, children: impl IntoIterator<Item = impl IntoElement>) -> Self {
        self.children
            .extend(children.into_iter().map(|c| c.into_any_element()));
        self
    }
}

impl Default for Card {
    fn default() -> Self {
        Self::new()
    }
}

impl Styled for Card {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Disableable for Card {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for Card {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let is_disabled = self.disabled;
        let is_clickable = self.on_click.is_some() && !is_disabled;

        let (bg, border_color, shadow_class) = match self.variant {
            CardVariant::Filled => (
                cx.theme().surface_container_highest,
                cx.theme().surface_container_highest,
                false,
            ),
            CardVariant::Elevated => (
                cx.theme().surface_container_low,
                cx.theme().surface_container_low,
                true,
            ),
            CardVariant::Outlined => (
                cx.theme().surface,
                cx.theme().outline_variant.opacity(0.6),
                false,
            ),
        };

        let hover_bg = if is_clickable {
            Some(bg.darken(0.04))
        } else {
            None
        };

        let container = v_flex()
            .p_4()
            .rounded_2xl()
            .bg(bg)
            .border_1()
            .border_color(border_color)
            .text_color(cx.theme().on_surface)
            .when(shadow_class, |this| this.shadow_md())
            .children(self.children);

        if let Some(id) = self.id {
            container
                .id(id)
                .when(is_clickable, |this| {
                    this.cursor_pointer()
                        .when_some(hover_bg, |this, bg| this.hover(|s| s.bg(bg)))
                })
                .when_some(self.on_click.filter(|_| !is_disabled), |this, handler| {
                    this.on_click(move |evt, window, cx| {
                        handler(evt, window, cx);
                    })
                })
                .into_any_element()
        } else {
            container.into_any_element()
        }
    }
}

/// Helper struct for Card header sections.
#[derive(IntoElement)]
pub struct CardHeader {
    title: SharedString,
    description: Option<SharedString>,
}

impl CardHeader {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            description: None,
        }
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }
}

impl RenderOnce for CardHeader {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        v_flex()
            .gap_1()
            .mb_2()
            .child(
                div()
                    .text_lg()
                    .font_bold()
                    .text_color(cx.theme().on_surface)
                    .child(self.title),
            )
            .when_some(self.description, |this, desc| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().on_surface_variant)
                        .child(desc),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_card_variants() {
        let filled = Card::filled();
        assert_eq!(filled.variant, CardVariant::Filled);

        let elevated = Card::elevated();
        assert_eq!(elevated.variant, CardVariant::Elevated);

        let outlined = Card::outlined();
        assert_eq!(outlined.variant, CardVariant::Outlined);
    }
}
