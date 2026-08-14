//! Style builder for configuring declarative node styles.

use crate::bindings::shilpo::extension::view::{Overflow, SemanticColorToken, ViewStyle};

/// Fluent builder for [`ViewStyle`].
#[derive(Clone, Debug, PartialEq)]
pub struct StyleBuilder {
    style: ViewStyle,
}

impl Default for StyleBuilder {
    fn default() -> Self {
        Self {
            style: ViewStyle {
                padding: None,
                margin: None,
                width: None,
                height: None,
                corner_radius: None,
                opacity: None,
                color: None,
                background: None,
                flex_grow: None,
                border_width: None,
                border_color: None,
                min_width: None,
                max_width: None,
                min_height: None,
                max_height: None,
                overflow: None,
            },
        }
    }
}

/// Creates a new empty [`StyleBuilder`].
///
/// # Examples
///
/// ```rust
/// use shilpo_ext_sdk::prelude::*;
///
/// let custom_style = style()
///     .padding(12.0)
///     .corner_radius(8.0)
///     .background(SemanticColorToken::SurfaceContainer)
///     .build();
///
/// assert_eq!(custom_style.padding, Some(12.0));
/// assert_eq!(custom_style.corner_radius, Some(8.0));
/// ```
pub fn style() -> StyleBuilder {
    StyleBuilder::default()
}

impl StyleBuilder {
    /// Sets the inner padding.
    pub fn padding(mut self, padding: f32) -> Self {
        self.style.padding = Some(padding);
        self
    }

    /// Sets the outer margin.
    pub fn margin(mut self, margin: f32) -> Self {
        self.style.margin = Some(margin);
        self
    }

    /// Sets the explicit width.
    pub fn width(mut self, width: f32) -> Self {
        self.style.width = Some(width);
        self
    }

    /// Sets the explicit height.
    pub fn height(mut self, height: f32) -> Self {
        self.style.height = Some(height);
        self
    }

    /// Sets both width and height simultaneously.
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.style.width = Some(width);
        self.style.height = Some(height);
        self
    }

    /// Sets the corner radius.
    pub fn corner_radius(mut self, radius: f32) -> Self {
        self.style.corner_radius = Some(radius);
        self
    }

    /// Sets the element opacity in range `0.0..=1.0`.
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.style.opacity = Some(opacity);
        self
    }

    /// Sets foreground color semantic token.
    pub fn color(mut self, color: SemanticColorToken) -> Self {
        self.style.color = Some(color);
        self
    }

    /// Sets background color semantic token.
    pub fn background(mut self, background: SemanticColorToken) -> Self {
        self.style.background = Some(background);
        self
    }

    /// Sets flex growth factor.
    pub fn flex_grow(mut self, flex_grow: f32) -> Self {
        self.style.flex_grow = Some(flex_grow);
        self
    }

    /// Sets border width.
    pub fn border_width(mut self, border_width: f32) -> Self {
        self.style.border_width = Some(border_width);
        self
    }

    /// Sets border color semantic token.
    pub fn border_color(mut self, border_color: SemanticColorToken) -> Self {
        self.style.border_color = Some(border_color);
        self
    }

    /// Sets both border width and color simultaneously.
    pub fn border(mut self, width: f32, color: SemanticColorToken) -> Self {
        self.style.border_width = Some(width);
        self.style.border_color = Some(color);
        self
    }

    /// Sets minimum width constraint.
    pub fn min_width(mut self, min_width: f32) -> Self {
        self.style.min_width = Some(min_width);
        self
    }

    /// Sets maximum width constraint.
    pub fn max_width(mut self, max_width: f32) -> Self {
        self.style.max_width = Some(max_width);
        self
    }

    /// Sets minimum height constraint.
    pub fn min_height(mut self, min_height: f32) -> Self {
        self.style.min_height = Some(min_height);
        self
    }

    /// Sets maximum height constraint.
    pub fn max_height(mut self, max_height: f32) -> Self {
        self.style.max_height = Some(max_height);
        self
    }

    /// Sets content overflow behavior.
    pub fn overflow(mut self, overflow: Overflow) -> Self {
        self.style.overflow = Some(overflow);
        self
    }

    /// Consumes the builder and returns the completed [`ViewStyle`].
    pub fn build(self) -> ViewStyle {
        self.style
    }
}

impl From<StyleBuilder> for ViewStyle {
    fn from(builder: StyleBuilder) -> Self {
        builder.build()
    }
}
