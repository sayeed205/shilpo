use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_height: Dp,
    pub container_shape_round: ShapeRole,
    pub container_shape_square: ShapeRole,
    pub icon_size: Dp,
    pub narrow_leading_space: Dp,
    pub narrow_trailing_space: Dp,
    pub outlined_outline_width: Dp,
    pub pressed_container_shape: ShapeRole,
    pub selected_container_shape_round: ShapeRole,
    pub selected_container_shape_square: ShapeRole,
    pub uniform_leading_space: Dp,
    pub uniform_trailing_space: Dp,
    pub wide_leading_space: Dp,
    pub wide_trailing_space: Dp,
}

/// Returns authored token values for this component.
pub const fn large_icon_button() -> Tokens {
    Tokens {
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42C00000)),
        container_shape_round: crate::material::tokens::shape::ShapeRole::CornerFull,
        container_shape_square: crate::material::tokens::shape::ShapeRole::CornerExtraLarge,
        icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x42000000)),
        narrow_leading_space: crate::material::tokens::units::Dp(f32::from_bits(0x41800000)),
        narrow_trailing_space: crate::material::tokens::units::Dp(f32::from_bits(0x41800000)),
        outlined_outline_width: crate::material::tokens::units::Dp(f32::from_bits(0x40000000)),
        pressed_container_shape: crate::material::tokens::shape::ShapeRole::CornerLarge,
        selected_container_shape_round: crate::material::tokens::shape::ShapeRole::CornerExtraLarge,
        selected_container_shape_square: crate::material::tokens::shape::ShapeRole::CornerFull,
        uniform_leading_space: crate::material::tokens::units::Dp(f32::from_bits(0x42000000)),
        uniform_trailing_space: crate::material::tokens::units::Dp(f32::from_bits(0x42000000)),
        wide_leading_space: crate::material::tokens::units::Dp(f32::from_bits(0x42400000)),
        wide_trailing_space: crate::material::tokens::units::Dp(f32::from_bits(0x42400000)),
    }
}
