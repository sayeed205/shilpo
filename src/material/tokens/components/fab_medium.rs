use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_height: Dp,
    pub container_width: Dp,
    pub icon_size: Dp,
}

/// Returns authored token values for this component.
pub const fn fab_medium() -> Tokens {
    Tokens {
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42A00000)),
        container_width: crate::material::tokens::units::Dp(f32::from_bits(0x42A00000)),
        icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41E00000)),
    }
}
