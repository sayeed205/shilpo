use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub active_indicator_color: ColorRole,
    pub active_size: Dp,
    pub contained_active_color: ColorRole,
    pub contained_container_color: ColorRole,
    pub container_height: Dp,
    pub container_shape: ShapeRole,
    pub container_width: Dp,
}

/// Returns authored token values for this component.
pub const fn loading_indicator() -> Tokens {
    Tokens {
        active_indicator_color: crate::material::tokens::color_role::ColorRole::Primary,
        active_size: crate::material::tokens::units::Dp(f32::from_bits(0x42180000)),
        contained_active_color: crate::material::tokens::color_role::ColorRole::OnPrimaryContainer,
        contained_container_color: crate::material::tokens::color_role::ColorRole::PrimaryContainer,
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42400000)),
        container_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        container_width: crate::material::tokens::units::Dp(f32::from_bits(0x42400000)),
    }
}
