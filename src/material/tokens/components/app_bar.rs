use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::foundation::ElevationRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub avatar_size: Dp,
    pub container_color: ColorRole,
    pub container_elevation: ElevationRole,
    pub container_shape: ShapeRole,
    pub icon_button_space: Dp,
    pub icon_size: Dp,
    pub leading_icon_color: ColorRole,
    pub leading_space: Dp,
    pub on_scroll_container_color: ColorRole,
    pub on_scroll_container_elevation: ElevationRole,
    pub subtitle_color: ColorRole,
    pub title_color: ColorRole,
    pub trailing_icon_color: ColorRole,
    pub trailing_space: Dp,
}

/// Returns authored token values for this component.
pub const fn app_bar() -> Tokens {
    Tokens {
        avatar_size: crate::material::tokens::units::Dp(f32::from_bits(0x42000000)),
        container_color: crate::material::tokens::color_role::ColorRole::Surface,
        container_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        container_shape: crate::material::tokens::shape::ShapeRole::CornerNone,
        icon_button_space: crate::material::tokens::units::Dp(f32::from_bits(0x00000000)),
        icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
        leading_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        leading_space: crate::material::tokens::units::Dp(f32::from_bits(0x40800000)),
        on_scroll_container_color: crate::material::tokens::color_role::ColorRole::SurfaceContainer,
        on_scroll_container_elevation: crate::material::tokens::foundation::ElevationRole::Level2,
        subtitle_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        title_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        trailing_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        trailing_space: crate::material::tokens::units::Dp(f32::from_bits(0x40800000)),
    }
}
