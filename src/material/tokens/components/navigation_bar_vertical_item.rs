use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub active_indicator_height: Dp,
    pub active_indicator_width: Dp,
    pub container_between_space: Dp,
    pub icon_size: Dp,
}

/// Returns authored token values for this component.
pub const fn navigation_bar_vertical_item() -> Tokens {
    Tokens {
        active_indicator_height: crate::material::tokens::units::Dp(f32::from_bits(0x42000000)),
        active_indicator_width: crate::material::tokens::units::Dp(f32::from_bits(0x42600000)),
        container_between_space: crate::material::tokens::units::Dp(f32::from_bits(0x40C00000)),
        icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
    }
}
