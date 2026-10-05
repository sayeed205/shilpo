use crate::material::tokens::shape::CornerValueRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub between_space: Dp,
    pub container_height: Dp,
    pub container_shape: ShapeRole,
    pub inner_corner_corner_size: CornerValueRole,
    pub inner_hovered_corner_corner_size: CornerValueRole,
    pub inner_pressed_corner_corner_size: CornerValueRole,
    pub leading_button_leading_space: Dp,
    pub leading_button_trailing_space: Dp,
    pub outer_corner_corner_size_percent: f32,
    pub trailing_icon_size: Dp,
    pub trailing_inner_selected_corner_corner_size_percent: f32,
    pub trailing_button_leading_space: Dp,
    pub trailing_button_trailing_space: Dp,
}

/// Returns authored token values for this component.
pub const fn split_button_x_small() -> Tokens {
    Tokens {
        between_space: crate::material::tokens::units::Dp(f32::from_bits(0x40000000)),
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42000000)),
        container_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        inner_corner_corner_size: crate::material::tokens::shape::CornerValueRole::CornerValueExtraSmall,
        inner_hovered_corner_corner_size:
            crate::material::tokens::shape::CornerValueRole::CornerValueSmall,
        inner_pressed_corner_corner_size:
            crate::material::tokens::shape::CornerValueRole::CornerValueSmall,
        leading_button_leading_space: crate::material::tokens::units::Dp(f32::from_bits(0x41400000)),
        leading_button_trailing_space: crate::material::tokens::units::Dp(f32::from_bits(0x41200000)),
        outer_corner_corner_size_percent: f32::from_bits(0x42480000),
        trailing_icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41B00000)),
        trailing_inner_selected_corner_corner_size_percent: f32::from_bits(0x42480000),
        trailing_button_leading_space: crate::material::tokens::units::Dp(f32::from_bits(0x41500000)),
        trailing_button_trailing_space: crate::material::tokens::units::Dp(f32::from_bits(0x41500000)),
    }
}
