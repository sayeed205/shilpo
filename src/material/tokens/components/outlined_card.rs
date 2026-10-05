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
    pub disabled_container_elevation: ElevationRole,
    pub disabled_outline_color: ColorRole,
    pub disabled_outline_opacity: Opacity,
    pub dragged_container_elevation: ElevationRole,
    pub dragged_outline_color: ColorRole,
    pub focus_container_elevation: ElevationRole,
    pub focus_outline_color: ColorRole,
    pub hover_container_elevation: ElevationRole,
    pub hover_outline_color: ColorRole,
    pub icon_color: ColorRole,
    pub icon_size: Dp,
    pub outline_color: ColorRole,
    pub outline_width: Dp,
    pub pressed_container_elevation: ElevationRole,
    pub pressed_outline_color: ColorRole,
}

/// Returns authored token values for this component.
pub const fn outlined_card() -> Tokens {
    Tokens {
        container_color: crate::material::tokens::color_role::ColorRole::Surface,
        container_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        container_shape: crate::material::tokens::shape::ShapeRole::CornerMedium,
        disabled_container_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        disabled_outline_color: crate::material::tokens::color_role::ColorRole::Outline,
        disabled_outline_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3DF5C28F)),
        dragged_container_elevation: crate::material::tokens::foundation::ElevationRole::Level3,
        dragged_outline_color: crate::material::tokens::color_role::ColorRole::OutlineVariant,
        focus_container_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        focus_outline_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        hover_container_elevation: crate::material::tokens::foundation::ElevationRole::Level1,
        hover_outline_color: crate::material::tokens::color_role::ColorRole::OutlineVariant,
        icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
        outline_color: crate::material::tokens::color_role::ColorRole::OutlineVariant,
        outline_width: crate::material::tokens::units::Dp(f32::from_bits(0x3F800000)),
        pressed_container_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        pressed_outline_color: crate::material::tokens::color_role::ColorRole::OutlineVariant,
    }
}
