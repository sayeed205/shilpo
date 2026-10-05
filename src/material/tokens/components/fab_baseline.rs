use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_height: Dp,
    pub container_shape: ShapeRole,
    pub container_width: Dp,
    pub icon_size: Dp,
}

/// Returns authored token values for this component.
pub const fn fab_baseline() -> Tokens {
    Tokens {
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42600000)),
        container_shape: crate::material::tokens::shape::ShapeRole::CornerLarge,
        container_width: crate::material::tokens::units::Dp(f32::from_bits(0x42600000)),
        icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
    }
}
