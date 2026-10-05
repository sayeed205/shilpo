use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::foundation::ElevationRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_color: ColorRole,
    pub container_elevation: ElevationRole,
    pub container_height: Dp,
    pub container_shape: ShapeRole,
}

/// Returns authored token values for this component.
pub const fn bottom_app_bar() -> Tokens {
    Tokens {
        container_color: crate::material::tokens::color_role::ColorRole::SurfaceContainer,
        container_elevation: crate::material::tokens::foundation::ElevationRole::Level2,
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42A00000)),
        container_shape: crate::material::tokens::shape::ShapeRole::CornerNone,
    }
}
