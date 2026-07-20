use gpui::{
    AnyElement, App, Hsla, IntoElement, ParentElement, RenderOnce, StyleRefinement, Styled, Window,
    div, prelude::FluentBuilder, px, relative,
};

use crate::{ActiveTheme, Icon, Sizable, Size, StyledExt, h_flex, white};

#[derive(Default, Clone)]
enum BadgeVariant {
    #[default]
    Number,
    Dot,
    Icon(Box<Icon>),
}

#[allow(unused)]
impl BadgeVariant {
    #[inline]
    fn is_icon(&self) -> bool {
        matches!(self, BadgeVariant::Icon(_))
    }

    #[inline]
    fn is_number(&self) -> bool {
        matches!(self, BadgeVariant::Number)
    }
}

/// A badge for displaying a count, dot, or icon on an element.
#[derive(IntoElement)]
pub struct Badge {
    style: StyleRefinement,
    count: usize,
    max: usize,
    variant: BadgeVariant,
    children: Vec<AnyElement>,
    color: Option<Hsla>,
    size: Size,
}

impl Badge {
    /// Create a new badge.
    pub fn new() -> Self {
        Self {
            style: StyleRefinement::default(),
            count: 0,
            max: 99,
            variant: Default::default(),
            color: None,
            children: Vec::new(),
            size: Size::default(),
        }
    }

    /// Set to use [`BadgeVariant::Dot`] to show a dot.
    pub fn dot(mut self) -> Self {
        self.variant = BadgeVariant::Dot;
        self
    }

    /// Set to use [`BadgeVariant::Number`] to show a count.
    ///
    /// If count is 0, the badge will be hidden.
    pub fn count(mut self, count: usize) -> Self {
        self.count = count;
        self
    }

    /// Set to use [`BadgeVariant::Icon`] to show an icon.
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.variant = BadgeVariant::Icon(Box::new(icon.into()));
        self
    }

    /// Set the maximum count to show (Only if [`BadgeVariant::Number`] is used).
    pub fn max(mut self, max: usize) -> Self {
        self.max = max;
        self
    }

    /// Set the color (background) of the badge.
    pub fn color(mut self, color: impl Into<Hsla>) -> Self {
        self.color = Some(color.into());
        self
    }
}

impl ParentElement for Badge {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Sizable for Badge {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl RenderOnce for Badge {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let visible = match self.variant {
            BadgeVariant::Number => self.count > 0,
            BadgeVariant::Dot | BadgeVariant::Icon(_) => true,
        };

        let (size, text_size) = match self.size {
            Size::Large => (px(24.), px(14.)),
            Size::Medium | Size::Size(_) => (px(16.), px(10.)),
            Size::Small | Size::XSmall => (px(10.), px(8.)),
        };

        div()
            .relative()
            .refine_style(&self.style)
            .children(self.children)
            .when(visible, |this| {
                this.child(
                    h_flex()
                        .absolute()
                        .justify_center()
                        .items_center()
                        .rounded_full()
                        .bg(self.color.unwrap_or(cx.theme().error))
                        .text_color(white())
                        .text_size(text_size)
                        .map(|this| match self.variant {
                            BadgeVariant::Dot => this.top_0().right_0().size(px(6.)),
                            BadgeVariant::Number => {
                                let count = if self.count > self.max {
                                    format!("{}+", self.max)
                                } else {
                                    self.count.to_string()
                                };

                                let (top, left) = match self.size {
                                    Size::Large => (px(2.), -px(count.len() as f32)),
                                    Size::Medium | Size::Size(_) => {
                                        (-px(3.), -px(3.) * count.len())
                                    }
                                    Size::Small | Size::XSmall => (-px(4.), -px(4.) * count.len()),
                                };

                                this.top(top)
                                    .right(left)
                                    .py_0p5()
                                    .px_0p5()
                                    .min_w_3p5()
                                    .text_size(px(10.))
                                    .line_height(relative(1.))
                                    .child(count)
                            }
                            BadgeVariant::Icon(icon) => this
                                .right_0()
                                .bottom_0()
                                .size(size)
                                .border_1()
                                .border_color(cx.theme().surface)
                                .child(*icon),
                        }),
                )
            })
    }
}

#[cfg(test)]
impl Badge {
    pub(crate) fn get_count(&self) -> usize {
        self.count
    }

    pub(crate) fn get_max(&self) -> usize {
        self.max
    }

    pub(crate) fn get_color(&self) -> Option<Hsla> {
        self.color
    }

    pub(crate) fn get_size(&self) -> Size {
        self.size
    }

    pub(crate) fn is_dot(&self) -> bool {
        matches!(self.variant, BadgeVariant::Dot)
    }

    pub(crate) fn is_number(&self) -> bool {
        matches!(self.variant, BadgeVariant::Number)
    }

    pub(crate) fn is_icon(&self) -> bool {
        matches!(self.variant, BadgeVariant::Icon(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_badge_variant_helpers() {
        let v_num = BadgeVariant::Number;
        assert!(v_num.is_number());
        assert!(!v_num.is_icon());

        let v_dot = BadgeVariant::Dot;
        assert!(!v_dot.is_number());
        assert!(!v_dot.is_icon());

        let v_icon = BadgeVariant::Icon(Box::new(Icon::empty()));
        assert!(!v_icon.is_number());
        assert!(v_icon.is_icon());
    }

    #[test]
    fn test_badge_builder() {
        let badge = Badge::new()
            .count(10)
            .max(50)
            .color(gpui::blue())
            .with_size(Size::Large);

        assert_eq!(badge.get_count(), 10);
        assert_eq!(badge.get_max(), 50);
        assert_eq!(badge.get_color(), Some(gpui::blue()));
        assert_eq!(badge.get_size(), Size::Large);
        assert!(badge.is_number());
    }

    #[test]
    fn test_badge_dot_variant() {
        let badge = Badge::new().dot();
        assert!(badge.is_dot());
        assert!(!badge.is_number());
        assert!(!badge.is_icon());
    }

    #[test]
    fn test_badge_icon_variant() {
        let badge = Badge::new().icon(Icon::empty());
        assert!(badge.is_icon());
        assert!(!badge.is_number());
        assert!(!badge.is_dot());
    }
}
