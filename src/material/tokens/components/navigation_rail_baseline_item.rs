use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub active_indicator_icon_label_space: Dp,
    pub active_indicator_leading_space: Dp,
    pub active_indicator_shape: ShapeRole,
    pub active_indicator_trailing_space: Dp,
    pub container_height: Dp,
    pub container_shape: ShapeRole,
    pub container_vertical_space: Dp,
    pub header_space_minimum: Dp,
    pub icon_size: Dp,
}

/// Returns authored token values for this component.
pub const fn navigation_rail_baseline_item() -> Tokens {
    Tokens {
        active_indicator_icon_label_space: crate::material::tokens::units::Dp(f32::from_bits(0x41000000)),
        active_indicator_leading_space: crate::material::tokens::units::Dp(f32::from_bits(0x41800000)),
        active_indicator_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        active_indicator_trailing_space: crate::material::tokens::units::Dp(f32::from_bits(0x41800000)),
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42800000)),
        container_shape: crate::material::tokens::shape::ShapeRole::CornerNone,
        container_vertical_space: crate::material::tokens::units::Dp(f32::from_bits(0x40C00000)),
        header_space_minimum: crate::material::tokens::units::Dp(f32::from_bits(0x42200000)),
        icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
    }
}
