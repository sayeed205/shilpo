use gpui::{Pixels, px};

use crate::Size;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SplitButtonShape {
    CornerFull,
    Corner(Pixels),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplitButtonShapes {
    pub shape: SplitButtonShape,
    pub hovered_shape: SplitButtonShape,
    pub pressed_shape: SplitButtonShape,
    pub checked_shape: SplitButtonShape,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplitButtonTokens {
    pub height: Pixels,
    pub leading_start: Pixels,
    pub leading_end: Pixels,
    pub trailing_start: Pixels,
    pub trailing_end: Pixels,
    pub icon: Pixels,
    pub min_width: Pixels,
    pub between_space: Pixels,
    pub inner_radius: Pixels,
    pub shapes: SplitButtonShapes,
}

pub fn tokens(size: Size) -> SplitButtonTokens {
    let (height, leading_start, leading_end, trailing_start, trailing_end, icon, inner, pressed) =
        match size {
            Size::XSmall => (32., 12., 10., 13., 13., 22., 4., 8.),
            Size::Small => (40., 16., 12., 13., 13., 22., 4., 12.),
            Size::Medium => (56., 24., 24., 15., 15., 26., 4., 12.),
            Size::Large => (96., 48., 48., 29., 29., 38., 8., 20.),
            Size::Size(value) if value <= px(32.) => {
                (value.as_f32(), 12., 10., 13., 13., 22., 4., 8.)
            }
            Size::Size(value) if value <= px(40.) => {
                (value.as_f32(), 16., 12., 13., 13., 22., 4., 12.)
            }
            Size::Size(value) if value <= px(56.) => {
                (value.as_f32(), 24., 24., 15., 15., 26., 4., 12.)
            }
            Size::Size(value) if value <= px(96.) => {
                (value.as_f32(), 48., 48., 29., 29., 38., 8., 20.)
            }
            Size::Size(value) => (value.as_f32(), 64., 64., 43., 43., 50., 12., 20.),
        };
    SplitButtonTokens {
        height: px(height),
        leading_start: px(leading_start),
        leading_end: px(leading_end),
        trailing_start: px(trailing_start),
        trailing_end: px(trailing_end),
        icon: px(icon),
        min_width: px(48.),
        between_space: px(2.),
        inner_radius: px(inner),
        shapes: SplitButtonShapes {
            shape: SplitButtonShape::CornerFull,
            hovered_shape: SplitButtonShape::Corner(px(pressed)),
            pressed_shape: SplitButtonShape::Corner(px(pressed)),
            // Checked shape is static AndroidX 50% shape metadata. Rendering must not morph.
            checked_shape: SplitButtonShape::CornerFull,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_x_split_button_tables_are_exact() {
        let xlarge = tokens(Size::Size(px(136.)));
        assert_eq!(xlarge.height, px(136.));
        assert_eq!(xlarge.leading_start, px(64.));
        assert_eq!(xlarge.leading_end, px(64.));
        assert_eq!(xlarge.trailing_start, px(43.));
        assert_eq!(xlarge.trailing_end, px(43.));
        assert_eq!(xlarge.icon, px(50.));
        assert_eq!(xlarge.min_width, px(48.));
        assert_eq!(xlarge.inner_radius, px(12.));
        assert_eq!(
            xlarge.shapes.pressed_shape,
            SplitButtonShape::Corner(px(20.))
        );
        assert_eq!(xlarge.shapes.checked_shape, SplitButtonShape::CornerFull);
        assert_eq!(xlarge.between_space, px(2.));
    }

    #[test]
    fn static_inner_and_pressed_radii_follow_size_scale() {
        let expected = [
            (Size::XSmall, 4., 8.),
            (Size::Small, 4., 12.),
            (Size::Medium, 4., 12.),
            (Size::Large, 8., 20.),
            (Size::Size(px(136.)), 12., 20.),
        ];
        for (size, inner, pressed) in expected {
            let tokens = tokens(size);
            assert_eq!(tokens.inner_radius, px(inner));
            assert_eq!(tokens.shapes.shape, SplitButtonShape::CornerFull);
            assert_eq!(
                tokens.shapes.hovered_shape,
                SplitButtonShape::Corner(px(pressed))
            );
            assert_eq!(
                tokens.shapes.pressed_shape,
                SplitButtonShape::Corner(px(pressed))
            );
            assert_eq!(tokens.shapes.checked_shape, SplitButtonShape::CornerFull);
            assert_eq!(tokens.between_space, px(2.));
        }
    }
}
