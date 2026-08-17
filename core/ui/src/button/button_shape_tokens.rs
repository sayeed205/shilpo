use gpui::Pixels;

use super::{ButtonRounded, button_dimension_tokens, button_scale_tokens};
use crate::Size;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ButtonShape {
    CornerFull,
    Corner(Pixels),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonShapes {
    pub shape: ButtonShape,
    pub pressed_shape: ButtonShape,
}

fn shape_row(size: Size) -> button_scale_tokens::ButtonShapeRow {
    let bucket = button_scale_tokens::size_metrics(size).shape_bucket;
    button_scale_tokens::button_shape_row(bucket)
}

pub fn button_shapes(size: Size) -> ButtonShapes {
    let row = shape_row(size);
    ButtonShapes {
        shape: ButtonShape::CornerFull,
        pressed_shape: ButtonShape::Corner(row.pressed),
    }
}

fn corner_radius(corner: ButtonShape, height: Pixels) -> Pixels {
    match corner {
        ButtonShape::CornerFull => height * 0.5,
        ButtonShape::Corner(value) => value,
    }
}

/// Resolves static M3/M3E shape tokens. Pressed-shape morphing is intentionally
/// not applied; state layers remain static.
pub(crate) fn resolve(rounding: ButtonRounded, size: Size, final_height: Option<Pixels>) -> Pixels {
    let shapes = button_shapes(size);
    let row = shape_row(size);
    match rounding {
        ButtonRounded::Token => corner_radius(
            shapes.shape,
            final_height.unwrap_or_else(|| button_dimension_tokens::height(size)),
        ),
        ButtonRounded::None => Pixels::ZERO,
        ButtonRounded::Small => {
            corner_radius(shapes.pressed_shape, button_dimension_tokens::height(size))
        }
        ButtonRounded::Medium => corner_radius(
            ButtonShape::Corner(row.square),
            button_dimension_tokens::height(size),
        ),
        ButtonRounded::Large => {
            corner_radius(shapes.pressed_shape, button_dimension_tokens::height(size))
        }
        ButtonRounded::Size(value) => value,
    }
}

pub(crate) fn resolve_pressed(
    rounding: ButtonRounded,
    size: Size,
    final_height: Option<Pixels>,
) -> Pixels {
    let shapes = button_shapes(size);
    match rounding {
        ButtonRounded::Token => corner_radius(
            shapes.pressed_shape,
            final_height.unwrap_or_else(|| button_dimension_tokens::height(size)),
        ),
        ButtonRounded::None => Pixels::ZERO,
        ButtonRounded::Size(value) => value * 0.5,
        other => resolve(other, size, final_height),
    }
}

#[cfg(test)]
mod tests {
    use gpui::px;

    use super::*;

    #[test]
    fn default_shapes_are_pills() {
        assert_eq!(resolve(ButtonRounded::Token, Size::Small, None), px(20.));
        assert_eq!(resolve(ButtonRounded::Token, Size::Medium, None), px(28.));
        assert_eq!(resolve(ButtonRounded::Token, Size::Large, None), px(48.));
        assert_eq!(
            resolve(ButtonRounded::Token, Size::Medium, Some(px(40.))),
            px(20.)
        );
    }

    #[test]
    fn explicit_shape_tokens_use_static_values() {
        assert_eq!(resolve(ButtonRounded::Small, Size::Small, None), px(8.));
        assert_eq!(resolve(ButtonRounded::Medium, Size::Small, None), px(12.));
        assert_eq!(resolve(ButtonRounded::Medium, Size::Medium, None), px(16.));
        assert_eq!(resolve(ButtonRounded::Large, Size::Large, None), px(16.));
    }

    #[test]
    fn custom_sizes_use_shape_buckets() {
        assert_eq!(
            button_shapes(Size::Size(px(33.))).pressed_shape,
            ButtonShape::Corner(px(8.))
        );
        assert_eq!(
            button_shapes(Size::Size(px(41.))).pressed_shape,
            ButtonShape::Corner(px(8.))
        );
        assert_eq!(
            button_shapes(Size::Size(px(57.))).pressed_shape,
            ButtonShape::Corner(px(12.))
        );
        assert_eq!(
            button_shapes(Size::Size(px(97.))).pressed_shape,
            ButtonShape::Corner(px(16.))
        );
        assert_eq!(
            button_shapes(Size::Size(px(116.))).pressed_shape,
            ButtonShape::Corner(px(16.))
        );
    }
}
