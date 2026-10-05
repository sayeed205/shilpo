use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::foundation::ElevationRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::units::{Dp, Opacity};

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_color: ColorRole,
    pub container_elevation: ElevationRole,
    pub container_shape: ShapeRole,
    pub disabled_container_color: ColorRole,
    pub disabled_container_elevation: ElevationRole,
    pub disabled_container_opacity: Opacity,
    pub dragged_container_elevation: ElevationRole,
    pub focus_container_elevation: ElevationRole,
    pub focus_indicator_color: ColorRole,
    pub hover_container_elevation: ElevationRole,
    pub icon_color: ColorRole,
    pub icon_size: Dp,
    pub pressed_container_elevation: ElevationRole,
}

/// Returns authored token values for this component.
pub const fn elevated_card() -> Tokens {
    Tokens {
        container_color: crate::material::tokens::color_role::ColorRole::SurfaceContainerLow,
        container_elevation: crate::material::tokens::foundation::ElevationRole::Level1,
        container_shape: crate::material::tokens::shape::ShapeRole::CornerMedium,
        disabled_container_color: crate::material::tokens::color_role::ColorRole::Surface,
        disabled_container_elevation: crate::material::tokens::foundation::ElevationRole::Level1,
        disabled_container_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3EC28F5C)),
        dragged_container_elevation: crate::material::tokens::foundation::ElevationRole::Level4,
        focus_container_elevation: crate::material::tokens::foundation::ElevationRole::Level1,
        focus_indicator_color: crate::material::tokens::color_role::ColorRole::Secondary,
        hover_container_elevation: crate::material::tokens::foundation::ElevationRole::Level2,
        icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
        pressed_container_elevation: crate::material::tokens::foundation::ElevationRole::Level1,
    }
}
