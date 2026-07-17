use gpui::{Pixels, px};
use crate::Size;

use super::{ButtonRounded, button_dimension_tokens};

#[derive(Clone, Copy)]
enum Corner {
    Full,
    Dp(f32),
}

#[derive(Clone, Copy)]
struct ShapeFamily {
    shape: Corner,
    pressed_shape: Corner,
    square: Corner,
}

const SMALL: ShapeFamily = ShapeFamily {
    shape: Corner::Full,
    pressed_shape: Corner::Dp(8.),
    square: Corner::Dp(12.),
};
const MEDIUM: ShapeFamily = ShapeFamily {
    shape: Corner::Full,
    pressed_shape: Corner::Dp(12.),
    square: Corner::Dp(16.),
};
const LARGE: ShapeFamily = ShapeFamily {
    shape: Corner::Full,
    pressed_shape: Corner::Dp(16.),
    square: Corner::Dp(28.),
};

fn family(size: Size) -> ShapeFamily {
    match size {
        Size::XSmall | Size::Small => SMALL,
        Size::Medium => MEDIUM,
        Size::Large | Size::Size(_) => LARGE,
    }
}

fn corner_radius(corner: Corner, height: Pixels) -> Pixels {
    match corner {
        Corner::Full => height * 0.5,
        Corner::Dp(value) => px(value),
    }
}

/// Resolves static M3/M3E shape tokens. Pressed-shape morphing is intentionally
/// not applied; state layers remain static.
pub(crate) fn resolve(
    rounding: ButtonRounded,
    size: Size,
    final_height: Option<Pixels>,
) -> Pixels {
    let family = family(size);
    let _pressed_shape = family.pressed_shape;
    match rounding {
        ButtonRounded::Token => corner_radius(
            family.shape,
            final_height.unwrap_or_else(|| button_dimension_tokens::height(size)),
        ),
        ButtonRounded::None => Pixels::ZERO,
        ButtonRounded::Small => corner_radius(family.pressed_shape, button_dimension_tokens::height(size)),
        ButtonRounded::Medium => corner_radius(family.square, button_dimension_tokens::height(size)),
        ButtonRounded::Large => corner_radius(family.pressed_shape, button_dimension_tokens::height(size)),
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
