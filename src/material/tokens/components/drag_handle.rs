use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::foundation::ElevationRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub color: ColorRole,
    pub container_width: Dp,
    pub dragged_color: ColorRole,
    pub dragged_elevation: ElevationRole,
    pub dragged_height: Dp,
    pub dragged_shape: ShapeRole,
    pub dragged_width: Dp,
    pub elevation: ElevationRole,
    pub height: Dp,
    pub pressed_color: ColorRole,
    pub pressed_elevation: ElevationRole,
    pub pressed_height: Dp,
    pub pressed_shape: ShapeRole,
    pub pressed_width: Dp,
    pub shape: ShapeRole,
    pub width: Dp,
}

/// Returns authored token values for this component.
pub const fn drag_handle() -> Tokens {
    Tokens {
        color: crate::material::tokens::color_role::ColorRole::Outline,
        container_width: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
        dragged_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        dragged_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        dragged_height: crate::material::tokens::units::Dp(f32::from_bits(0x42500000)),
        dragged_shape: crate::material::tokens::shape::ShapeRole::CornerMedium,
        dragged_width: crate::material::tokens::units::Dp(f32::from_bits(0x41400000)),
        elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        height: crate::material::tokens::units::Dp(f32::from_bits(0x42400000)),
        pressed_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        pressed_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        pressed_height: crate::material::tokens::units::Dp(f32::from_bits(0x42500000)),
        pressed_shape: crate::material::tokens::shape::ShapeRole::CornerMedium,
        pressed_width: crate::material::tokens::units::Dp(f32::from_bits(0x41400000)),
        shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        width: crate::material::tokens::units::Dp(f32::from_bits(0x40800000)),
    }
}
