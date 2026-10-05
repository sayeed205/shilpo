use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_height: Dp,
    pub container_shape: ShapeRole,
    pub icon_label_space: Dp,
    pub icon_size: Dp,
    pub leading_space: Dp,
    pub trailing_space: Dp,
}

/// Returns authored token values for this component.
pub const fn extended_fab_small() -> Tokens {
    Tokens {
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42600000)),
        container_shape: crate::material::tokens::shape::ShapeRole::CornerLarge,
        icon_label_space: crate::material::tokens::units::Dp(f32::from_bits(0x41000000)),
        icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
        leading_space: crate::material::tokens::units::Dp(f32::from_bits(0x41800000)),
        trailing_space: crate::material::tokens::units::Dp(f32::from_bits(0x41800000)),
    }
}
