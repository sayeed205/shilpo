use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_color: ColorRole,
    pub container_height: Dp,
    pub container_leading_space: Dp,
    pub container_max_spacing: Dp,
    pub container_min_spacing: Dp,
    pub container_shape: ShapeRole,
    pub container_trailing_space: Dp,
}

/// Returns authored token values for this component.
pub const fn docked_toolbar() -> Tokens {
    Tokens {
        container_color: crate::material::tokens::color_role::ColorRole::SurfaceContainer,
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42800000)),
        container_leading_space: crate::material::tokens::units::Dp(f32::from_bits(0x41800000)),
        container_max_spacing: crate::material::tokens::units::Dp(f32::from_bits(0x42000000)),
        container_min_spacing: crate::material::tokens::units::Dp(f32::from_bits(0x40800000)),
        container_shape: crate::material::tokens::shape::ShapeRole::CornerNone,
        container_trailing_space: crate::material::tokens::units::Dp(f32::from_bits(0x41800000)),
    }
}
