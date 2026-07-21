use gpui::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement, ParentElement, RenderOnce,
    StyleRefinement, Styled, Window, div, px,
};

use crate::{ActiveTheme, Colorize, Sizable, Size, h_flex, v_flex};

/// Layout orientation for the floating toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FloatingToolbarOrientation {
    /// Horizontal row layout.
    #[default]
    Horizontal,
    /// Vertical column layout.
    Vertical,
}

/// Visual styling of the floating toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FloatingToolbarStyle {
    /// Standard surface container with neutral tones.
    #[default]
    Standard,
    /// Vibrant primary/secondary container with accent tones.
    Vibrant,
}

/// A Material Design 3 Expressive FloatingToolbar component.
///
/// Floating toolbars display context-specific actions and controls floating above page content.
///
/// # Reference
/// AndroidX `FloatingToolbar.kt` — `HorizontalFloatingToolbar`, `VerticalFloatingToolbar`.
#[derive(IntoElement)]
pub struct FloatingToolbar {
    id: ElementId,
    orientation: FloatingToolbarOrientation,
    toolbar_style: FloatingToolbarStyle,
    size: Size,
    expanded: bool,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl FloatingToolbar {
    /// Creates a new FloatingToolbar with default horizontal orientation.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            orientation: FloatingToolbarOrientation::Horizontal,
            toolbar_style: FloatingToolbarStyle::Standard,
            size: Size::Medium,
            expanded: true,
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// Creates a horizontal FloatingToolbar.
    pub fn horizontal(id: impl Into<ElementId>) -> Self {
        Self::new(id).orientation(FloatingToolbarOrientation::Horizontal)
    }

    /// Creates a vertical FloatingToolbar.
    pub fn vertical(id: impl Into<ElementId>) -> Self {
        Self::new(id).orientation(FloatingToolbarOrientation::Vertical)
    }

    /// Sets the layout orientation (Horizontal or Vertical).
    pub fn orientation(mut self, orientation: FloatingToolbarOrientation) -> Self {
        self.orientation = orientation;
        self
    }

    /// Sets the visual style (Standard or Vibrant).
    pub fn toolbar_style(mut self, toolbar_style: FloatingToolbarStyle) -> Self {
        self.toolbar_style = toolbar_style;
        self
    }

    /// Sets whether to use vibrant styling.
    pub fn vibrant(mut self, vibrant: bool) -> Self {
        self.toolbar_style = if vibrant {
            FloatingToolbarStyle::Vibrant
        } else {
            FloatingToolbarStyle::Standard
        };
        self
    }

    /// Sets the expanded state.
    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    /// Appends a single child element to the toolbar.
    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }

    /// Appends multiple child elements to the toolbar.
    pub fn children(mut self, children: impl IntoIterator<Item = impl IntoElement>) -> Self {
        self.children
            .extend(children.into_iter().map(|c| c.into_any_element()));
        self
    }
}

impl Styled for FloatingToolbar {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Sizable for FloatingToolbar {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl RenderOnce for FloatingToolbar {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let is_vibrant = self.toolbar_style == FloatingToolbarStyle::Vibrant;

        let (padding, gap, min_height, _rounded_r) = match self.size {
            Size::XSmall => (px(4.), px(4.), px(32.), px(16.)),
            Size::Small => (px(6.), px(6.), px(40.), px(20.)),
            Size::Medium => (px(8.), px(8.), px(48.), px(24.)),
            Size::Large => (px(12.), px(12.), px(56.), px(28.)),
            Size::Size(s) => (px(8.), px(8.), s, s * 0.5),
        };

        let (bg, border_color) = if is_vibrant {
            (
                cx.theme().primary_container,
                cx.theme().primary_container.darken(0.05),
            )
        } else {
            (
                cx.theme().surface_container_high,
                cx.theme().outline_variant.opacity(0.4),
            )
        };

        let container = div()
            .id(self.id)
            .p(padding)
            .rounded_full()
            .bg(bg)
            .border_1()
            .border_color(border_color)
            .shadow_lg();

        match self.orientation {
            FloatingToolbarOrientation::Horizontal => container.child(
                h_flex()
                    .items_center()
                    .min_h(min_height)
                    .gap(gap)
                    .children(self.children),
            ),
            FloatingToolbarOrientation::Vertical => container.child(
                v_flex()
                    .items_center()
                    .min_w(min_height)
                    .gap(gap)
                    .children(self.children),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_floating_toolbar_orientations() {
        let h = FloatingToolbar::horizontal("ft-1");
        assert_eq!(h.orientation, FloatingToolbarOrientation::Horizontal);

        let v = FloatingToolbar::vertical("ft-2");
        assert_eq!(v.orientation, FloatingToolbarOrientation::Vertical);
    }
}
