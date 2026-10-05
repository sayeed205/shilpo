use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub between_space: Dp,
    pub container_height: Dp,
}

/// Returns authored token values for this component.
pub const fn button_group_small() -> Tokens {
    Tokens {
        between_space: crate::material::tokens::units::Dp(f32::from_bits(0x41400000)),
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42200000)),
    }
}
