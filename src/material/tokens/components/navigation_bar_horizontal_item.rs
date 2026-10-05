use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub active_indicator_height: Dp,
    pub active_indicator_leading_space: Dp,
    pub active_indicator_trailing_space: Dp,
    pub icon_size: Dp,
}

/// Returns authored token values for this component.
pub const fn navigation_bar_horizontal_item() -> Tokens {
    Tokens {
        active_indicator_height: crate::material::tokens::units::Dp(f32::from_bits(0x42200000)),
        active_indicator_leading_space: crate::material::tokens::units::Dp(f32::from_bits(0x41800000)),
        active_indicator_trailing_space: crate::material::tokens::units::Dp(f32::from_bits(0x41800000)),
        icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
    }
}
