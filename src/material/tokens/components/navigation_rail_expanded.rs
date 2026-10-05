use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::foundation::ElevationRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_elevation: ElevationRole,
    pub container_shape: ShapeRole,
    pub container_width_maximum: Dp,
    pub container_width_minimum: Dp,
    pub modal_container_elevation: ElevationRole,
    pub modal_container_shape: ShapeRole,
    pub top_space: Dp,
    pub modal_container_color: ColorRole,
}

/// Returns authored token values for this component.
pub const fn navigation_rail_expanded() -> Tokens {
    Tokens {
        container_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        container_shape: crate::material::tokens::shape::ShapeRole::CornerNone,
        container_width_maximum: crate::material::tokens::units::Dp(f32::from_bits(0x43B40000)),
        container_width_minimum: crate::material::tokens::units::Dp(f32::from_bits(0x435C0000)),
        modal_container_elevation: crate::material::tokens::foundation::ElevationRole::Level2,
        modal_container_shape: crate::material::tokens::shape::ShapeRole::CornerLarge,
        top_space: crate::material::tokens::units::Dp(f32::from_bits(0x42300000)),
        modal_container_color: crate::material::tokens::color_role::ColorRole::SurfaceContainer,
    }
}
