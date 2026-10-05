use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_height: Dp,
    pub icon_label_space: Dp,
    pub icon_size: Dp,
    pub leading_space: Dp,
    pub trailing_space: Dp,
}

/// Returns authored token values for this component.
pub const fn extended_fab_medium() -> Tokens {
    Tokens {
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42A00000)),
        icon_label_space: crate::material::tokens::units::Dp(f32::from_bits(0x41800000)),
        icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41E00000)),
        leading_space: crate::material::tokens::units::Dp(f32::from_bits(0x41D00000)),
        trailing_space: crate::material::tokens::units::Dp(f32::from_bits(0x41D00000)),
    }
}
