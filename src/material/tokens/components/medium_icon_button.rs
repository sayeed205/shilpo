use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_height: Dp,
    pub container_shape_round: ShapeRole,
    pub container_shape_square: ShapeRole,
    pub default_leading_space: Dp,
    pub default_trailing_space: Dp,
    pub icon_size: Dp,
    pub narrow_leading_space: Dp,
    pub narrow_trailing_space: Dp,
    pub outlined_outline_width: Dp,
    pub pressed_container_shape: ShapeRole,
    pub selected_container_shape_round: ShapeRole,
    pub selected_container_shape_square: ShapeRole,
    pub wide_leading_space: Dp,
    pub wide_trailing_space: Dp,
}

/// Returns authored token values for this component.
pub const fn medium_icon_button() -> Tokens {
    Tokens {
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42600000)),
        container_shape_round: crate::material::tokens::shape::ShapeRole::CornerFull,
        container_shape_square: crate::material::tokens::shape::ShapeRole::CornerLarge,
        default_leading_space: crate::material::tokens::units::Dp(f32::from_bits(0x41800000)),
        default_trailing_space: crate::material::tokens::units::Dp(f32::from_bits(0x41800000)),
        icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
        narrow_leading_space: crate::material::tokens::units::Dp(f32::from_bits(0x41400000)),
        narrow_trailing_space: crate::material::tokens::units::Dp(f32::from_bits(0x41400000)),
        outlined_outline_width: crate::material::tokens::units::Dp(f32::from_bits(0x3F800000)),
        pressed_container_shape: crate::material::tokens::shape::ShapeRole::CornerMedium,
        selected_container_shape_round: crate::material::tokens::shape::ShapeRole::CornerLarge,
        selected_container_shape_square: crate::material::tokens::shape::ShapeRole::CornerFull,
        wide_leading_space: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
        wide_trailing_space: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
    }
}
