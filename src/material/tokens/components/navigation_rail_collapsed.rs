use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::foundation::ElevationRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_elevation: ElevationRole,
    pub container_shape: ShapeRole,
    pub container_width: Dp,
    pub item_vertical_space: Dp,
    pub top_space: Dp,
    pub container_color: ColorRole,
    pub narrow_container_width: Dp,
}

/// Returns authored token values for this component.
pub const fn navigation_rail_collapsed() -> Tokens {
    Tokens {
        container_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        container_shape: crate::material::tokens::shape::ShapeRole::CornerNone,
        container_width: crate::material::tokens::units::Dp(f32::from_bits(0x42C00000)),
        item_vertical_space: crate::material::tokens::units::Dp(f32::from_bits(0x40800000)),
        top_space: crate::material::tokens::units::Dp(f32::from_bits(0x42300000)),
        container_color: crate::material::tokens::color_role::ColorRole::Surface,
        narrow_container_width: crate::material::tokens::units::Dp(f32::from_bits(0x42A00000)),
    }
}
