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
pub const fn extended_fab_large() -> Tokens {
    Tokens {
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42C00000)),
        container_shape: crate::material::tokens::shape::ShapeRole::CornerExtraLarge,
        icon_label_space: crate::material::tokens::units::Dp(f32::from_bits(0x41A00000)),
        icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x42000000)),
        leading_space: crate::material::tokens::units::Dp(f32::from_bits(0x41E00000)),
        trailing_space: crate::material::tokens::units::Dp(f32::from_bits(0x41E00000)),
    }
}
