use crate::Size;
use gpui::{Pixels, px};

use super::{ButtonRounded, button_dimension_tokens};

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

#[derive(Clone, Copy)]
struct ShapeFamily {
    shapes: ButtonShapes,
    square: ButtonShape,
}

const SMALL: ShapeFamily = ShapeFamily {
    shapes: ButtonShapes {
        shape: ButtonShape::CornerFull,
        pressed_shape: ButtonShape::Corner(px(8.)),
    },
    square: ButtonShape::Corner(px(12.)),
};
const MEDIUM: ShapeFamily = ShapeFamily {
    shapes: ButtonShapes {
        shape: ButtonShape::CornerFull,
        pressed_shape: ButtonShape::Corner(px(12.)),
    },
    square: ButtonShape::Corner(px(16.)),
};
const LARGE: ShapeFamily = ShapeFamily {
    shapes: ButtonShapes {
        shape: ButtonShape::CornerFull,
        pressed_shape: ButtonShape::Corner(px(16.)),
    },
    square: ButtonShape::Corner(px(28.)),
};

fn family(size: Size) -> ShapeFamily {
    match size {
        Size::XSmall | Size::Small => SMALL,
        Size::Medium => MEDIUM,
        Size::Large | Size::Size(_) => LARGE,
    }
}

pub fn button_shapes(size: Size) -> ButtonShapes {
    family(size).shapes
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
    let family = family(size);
    let _pressed_shape = family.shapes.pressed_shape;
    match rounding {
        ButtonRounded::Token => corner_radius(
            family.shapes.shape,
            final_height.unwrap_or_else(|| button_dimension_tokens::height(size)),
        ),
        ButtonRounded::None => Pixels::ZERO,
        ButtonRounded::Small => corner_radius(
            family.shapes.pressed_shape,
            button_dimension_tokens::height(size),
        ),
        ButtonRounded::Medium => {
            corner_radius(family.square, button_dimension_tokens::height(size))
        }
        ButtonRounded::Large => corner_radius(
            family.shapes.pressed_shape,
            button_dimension_tokens::height(size),
        ),
        ButtonRounded::Size(value) => value,
    }
}

#[cfg(test)]
mod tests {
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
}
