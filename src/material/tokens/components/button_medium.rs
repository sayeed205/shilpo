use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_height: Dp,
    pub container_shape_round: ShapeRole,
    pub container_shape_square: ShapeRole,
    pub icon_label_space: Dp,
    pub icon_size: Dp,
    pub leading_space: Dp,
    pub outlined_outline_width: Dp,
    pub pressed_container_shape: ShapeRole,
    pub selected_container_shape_round: ShapeRole,
    pub selected_container_shape_square: ShapeRole,
    pub trailing_space: Dp,
}

/// Returns authored token values for this component.
pub const fn button_medium() -> Tokens {
    Tokens {
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42600000)),
        container_shape_round: crate::material::tokens::shape::ShapeRole::CornerFull,
        container_shape_square: crate::material::tokens::shape::ShapeRole::CornerLarge,
        icon_label_space: crate::material::tokens::units::Dp(f32::from_bits(0x41000000)),
        icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
        leading_space: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
        outlined_outline_width: crate::material::tokens::units::Dp(f32::from_bits(0x3F800000)),
        pressed_container_shape: crate::material::tokens::shape::ShapeRole::CornerMedium,
        selected_container_shape_round: crate::material::tokens::shape::ShapeRole::CornerFull,
        selected_container_shape_square: crate::material::tokens::shape::ShapeRole::CornerLarge,
        trailing_space: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
    }
}
