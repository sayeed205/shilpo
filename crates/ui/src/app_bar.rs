use gpui::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement, ParentElement, RenderOnce,
    StyleRefinement, Styled, Window, px,
};

use crate::{ActiveTheme, StyledExt, h_flex, v_flex};

/// Material 3 Expressive TopAppBar variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TopAppBarVariant {
    /// Standard small TopAppBar with left-aligned title.
    #[default]
    Small,
    /// Center-aligned title TopAppBar.
    CenterAligned,
    /// Medium height TopAppBar with prominent subtitle.
    Medium,
    /// Large flexible height TopAppBar.
    Large,
}

/// A Material Design 3 Expressive TopAppBar component for desktop window header and screen navigation.
///
/// # Reference
/// AndroidX `AppBar.kt` — `TopAppBar`, `CenterAlignedTopAppBar`, `MediumTopAppBar`, `LargeTopAppBar`.
#[derive(IntoElement)]
pub struct TopAppBar {
    id: ElementId,
    variant: TopAppBarVariant,
    title: Option<AnyElement>,
    navigation_icon: Option<AnyElement>,
    actions: Vec<AnyElement>,
    style: StyleRefinement,
}

impl TopAppBar {
    /// Creates a new TopAppBar.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            variant: TopAppBarVariant::Small,
            title: None,
            navigation_icon: None,
            actions: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// Creates a small TopAppBar.
    pub fn small(id: impl Into<ElementId>) -> Self {
        Self::new(id).variant(TopAppBarVariant::Small)
    }

    /// Creates a center-aligned TopAppBar.
    pub fn center_aligned(id: impl Into<ElementId>) -> Self {
        Self::new(id).variant(TopAppBarVariant::CenterAligned)
    }

    /// Creates a medium TopAppBar.
    pub fn medium(id: impl Into<ElementId>) -> Self {
        Self::new(id).variant(TopAppBarVariant::Medium)
    }

    /// Creates a large TopAppBar.
    pub fn large(id: impl Into<ElementId>) -> Self {
        Self::new(id).variant(TopAppBarVariant::Large)
    }

    /// Sets the variant.
    pub fn variant(mut self, variant: TopAppBarVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Sets the title element or text.
    pub fn title(mut self, title: impl IntoElement) -> Self {
        self.title = Some(title.into_any_element());
        self
    }

    /// Sets the leading navigation icon element (e.g. back arrow or drawer menu).
    pub fn navigation_icon(mut self, icon: impl IntoElement) -> Self {
        self.navigation_icon = Some(icon.into_any_element());
        self
    }

    /// Appends a trailing action button or element.
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.actions.push(action.into_any_element());
        self
    }

    /// Appends multiple trailing action elements.
    pub fn actions(mut self, actions: impl IntoIterator<Item = impl IntoElement>) -> Self {
        self.actions
            .extend(actions.into_iter().map(|a| a.into_any_element()));
        self
    }
}

impl Styled for TopAppBar {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for TopAppBar {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (height, font_size, is_stacked) = match self.variant {
            TopAppBarVariant::Small | TopAppBarVariant::CenterAligned => (px(64.), px(18.), false),
            TopAppBarVariant::Medium => (px(112.), px(22.), true),
            TopAppBarVariant::Large => (px(152.), px(28.), true),
        };

        let bg = cx.theme().surface_container;
        let fg = cx.theme().on_surface;

        let nav = self.navigation_icon;
        let title_elem = self.title;
        let actions = self.actions;

        if is_stacked {
            // Medium / Large stacked layout
            v_flex()
                .id(self.id)
                .w_full()
                .h(height)
                .bg(bg)
                .text_color(fg)
                .px_4()
                .py_3()
                .justify_between()
                .child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .justify_between()
                        .child(h_flex().items_center().gap_3().children(nav))
                        .child(h_flex().items_center().gap_2().children(actions)),
                )
                .child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .text_size(font_size)
                        .font_bold()
                        .children(title_elem),
                )
        } else if self.variant == TopAppBarVariant::CenterAligned {
            // Center aligned single-row layout
            h_flex()
                .id(self.id)
                .w_full()
                .h(height)
                .bg(bg)
                .text_color(fg)
                .px_4()
                .items_center()
                .justify_between()
                .child(h_flex().items_center().gap_3().children(nav))
                .child(
                    h_flex()
                        .items_center()
                        .justify_center()
                        .text_size(font_size)
                        .font_bold()
                        .children(title_elem),
                )
                .child(h_flex().items_center().gap_2().children(actions))
        } else {
            // Standard small single-row layout
            h_flex()
                .id(self.id)
                .w_full()
                .h(height)
                .bg(bg)
                .text_color(fg)
                .px_4()
                .items_center()
                .gap_4()
                .children(nav)
                .child(
                    h_flex()
                        .flex_1()
                        .items_center()
                        .text_size(font_size)
                        .font_bold()
                        .children(title_elem),
                )
                .child(h_flex().items_center().gap_2().children(actions))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_bar_variants() {
        let small = TopAppBar::small("ab-1");
        assert_eq!(small.variant, TopAppBarVariant::Small);

        let center = TopAppBar::center_aligned("ab-2");
        assert_eq!(center.variant, TopAppBarVariant::CenterAligned);

        let medium = TopAppBar::medium("ab-3");
        assert_eq!(medium.variant, TopAppBarVariant::Medium);

        let large = TopAppBar::large("ab-4");
        assert_eq!(large.variant, TopAppBarVariant::Large);
    }
}
