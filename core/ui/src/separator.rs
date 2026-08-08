use crate::{ActiveTheme, StyledExt};
use gpui::{
    App, Axis, Div, Hsla, IntoElement, ParentElement, PathBuilder, RenderOnce, SharedString,
    StyleRefinement, Styled, Window, canvas, div, point, prelude::FluentBuilder as _, px,
};

/// The style of the separator line.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SeparatorStyle {
    #[default]
    Solid,
    Dashed,
}

/// A separator that can be either vertical or horizontal.
#[derive(IntoElement)]
pub struct Separator {
    base: Div,
    style: StyleRefinement,
    label: Option<SharedString>,
    axis: Axis,
    color: Option<Hsla>,
    line_style: SeparatorStyle,
}

impl Separator {
    /// Creates a vertical separator.
    pub fn vertical() -> Self {
        Self {
            base: div().h_full(),
            axis: Axis::Vertical,
            label: None,
            color: None,
            style: StyleRefinement::default(),
            line_style: SeparatorStyle::Solid,
        }
    }

    /// Creates a horizontal separator.
    pub fn horizontal() -> Self {
        Self {
            base: div(),
            axis: Axis::Horizontal,
            label: None,
            color: None,
            style: StyleRefinement::default(),
            line_style: SeparatorStyle::Solid,
        }
    }

    /// Creates a vertical dashed separator.
    pub fn vertical_dashed() -> Self {
        Self::vertical().dashed()
    }

    /// Creates a horizontal dashed separator.
    pub fn horizontal_dashed() -> Self {
        Self::horizontal().dashed()
    }

    /// Sets the label for the separator.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Sets the color for the separator line.
    pub fn color(mut self, color: impl Into<Hsla>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Sets the style of the separator to dashed.
    pub fn dashed(mut self) -> Self {
        self.line_style = SeparatorStyle::Dashed;
        self
    }

    fn render_base(axis: Axis) -> Div {
        div().absolute().map(|this| match axis {
            Axis::Vertical => this.w(px(1.)).h_full(),
            Axis::Horizontal => this.h(px(1.)).w_full(),
        })
    }

    fn render_solid(axis: Axis, color: Hsla) -> impl IntoElement {
        Self::render_base(axis).bg(color)
    }

    fn render_dashed(axis: Axis, color: Hsla) -> impl IntoElement {
        Self::render_base(axis).child(
            canvas(
                move |_, _, _| {},
                move |bounds, _, window, _| {
                    let mut builder = PathBuilder::stroke(px(1.)).dash_array(&[px(4.), px(2.)]);
                    let (start, end) = match axis {
                        Axis::Horizontal => {
                            let x = bounds.origin.x;
                            let y = bounds.origin.y + px(0.5);
                            (point(x, y), point(x + bounds.size.width, y))
                        }
                        Axis::Vertical => {
                            let x = bounds.origin.x + px(0.5);
                            let y = bounds.origin.y;
                            (point(x, y), point(x, y + bounds.size.height))
                        }
                    };
                    builder.move_to(start);
                    builder.line_to(end);
                    if let Ok(line) = builder.build() {
                        window.paint_path(line, color);
                    }
                },
            )
            .size_full(),
        )
    }
}

impl Styled for Separator {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Separator {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let color = self.color.unwrap_or(cx.theme().outline_variant);
        let axis = self.axis;
        let line_style = self.line_style;

        self.base
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .refine_style(&self.style)
            .child(match line_style {
                SeparatorStyle::Solid => Self::render_solid(axis, color).into_any_element(),
                SeparatorStyle::Dashed => Self::render_dashed(axis, color).into_any_element(),
            })
            .when_some(self.label, |this, label| {
                this.child(
                    div()
                        .px_2()
                        .py_1()
                        .mx_auto()
                        .text_xs()
                        .bg(cx.theme().surface)
                        .text_color(cx.theme().on_surface_variant)
                        .child(label),
                )
            })
    }
}

#[cfg(test)]
impl Separator {
    pub(crate) fn get_axis(&self) -> Axis {
        self.axis
    }

    pub(crate) fn get_label(&self) -> Option<SharedString> {
        self.label.clone()
    }

    pub(crate) fn get_color(&self) -> Option<Hsla> {
        self.color
    }

    pub(crate) fn get_line_style(&self) -> SeparatorStyle {
        self.line_style
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_separator_constructors() {
        let sep_vert = Separator::vertical();
        assert_eq!(sep_vert.get_axis(), Axis::Vertical);
        assert_eq!(sep_vert.get_line_style(), SeparatorStyle::Solid);

        let sep_horiz = Separator::horizontal();
        assert_eq!(sep_horiz.get_axis(), Axis::Horizontal);
        assert_eq!(sep_horiz.get_line_style(), SeparatorStyle::Solid);

        let sep_vert_dash = Separator::vertical_dashed();
        assert_eq!(sep_vert_dash.get_axis(), Axis::Vertical);
        assert_eq!(sep_vert_dash.get_line_style(), SeparatorStyle::Dashed);

        let sep_horiz_dash = Separator::horizontal_dashed();
        assert_eq!(sep_horiz_dash.get_axis(), Axis::Horizontal);
        assert_eq!(sep_horiz_dash.get_line_style(), SeparatorStyle::Dashed);
    }

    #[test]
    fn test_separator_builder() {
        let sep = Separator::horizontal()
            .label("OR")
            .color(gpui::blue())
            .dashed();

        assert_eq!(sep.get_label(), Some("OR".into()));
        assert_eq!(sep.get_color(), Some(gpui::blue()));
        assert_eq!(sep.get_line_style(), SeparatorStyle::Dashed);
    }
}
