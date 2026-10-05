use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::foundation::ElevationRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::typography::TypeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub active_label_text_color: ColorRole,
    pub container_color: ColorRole,
    pub container_elevation: ElevationRole,
    pub container_height: Dp,
    pub container_shape: ShapeRole,
    pub divider_color: ColorRole,
    pub divider_height: Dp,
    pub focus_label_text_color: ColorRole,
    pub hover_label_text_color: ColorRole,
    pub inactive_label_text_color: ColorRole,
    pub label_text_font: TypeRole,
    pub pressed_label_text_color: ColorRole,
    pub active_icon_color: ColorRole,
    pub focus_icon_color: ColorRole,
    pub hover_icon_color: ColorRole,
    pub icon_size: Dp,
    pub inactive_icon_color: ColorRole,
    pub pressed_icon_color: ColorRole,
}

/// Returns authored token values for this component.
pub const fn secondary_navigation_tab() -> Tokens {
    Tokens {
        active_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        container_color: crate::material::tokens::color_role::ColorRole::Surface,
        container_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42400000)),
        container_shape: crate::material::tokens::shape::ShapeRole::CornerNone,
        divider_color: crate::material::tokens::color_role::ColorRole::SurfaceVariant,
        divider_height: crate::material::tokens::units::Dp(f32::from_bits(0x3F800000)),
        focus_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        hover_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        inactive_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        label_text_font: crate::material::tokens::typography::TypeRole::TitleSmall,
        pressed_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        active_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        focus_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        hover_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
        inactive_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        pressed_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
    }
}
