use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::foundation::ElevationRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::typography::TypeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub avatar_shape: ShapeRole,
    pub avatar_size: Dp,
    pub container_color: ColorRole,
    pub container_elevation: ElevationRole,
    pub container_height: Dp,
    pub container_shape: ShapeRole,
    pub focus_indicator_color: ColorRole,
    pub hover_supporting_text_color: ColorRole,
    pub input_text_color: ColorRole,
    pub input_text_font: TypeRole,
    pub leading_icon_color: ColorRole,
    pub pressed_supporting_text_color: ColorRole,
    pub supporting_text_color: ColorRole,
    pub supporting_text_font: TypeRole,
    pub trailing_icon_color: ColorRole,
}

/// Returns authored token values for this component.
pub const fn search_bar() -> Tokens {
    Tokens {
        avatar_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        avatar_size: crate::material::tokens::units::Dp(f32::from_bits(0x41F00000)),
        container_color: crate::material::tokens::color_role::ColorRole::SurfaceContainerHigh,
        container_elevation: crate::material::tokens::foundation::ElevationRole::Level3,
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42600000)),
        container_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        focus_indicator_color: crate::material::tokens::color_role::ColorRole::Secondary,
        hover_supporting_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        input_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        input_text_font: crate::material::tokens::typography::TypeRole::BodyLarge,
        leading_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        pressed_supporting_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        supporting_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        supporting_text_font: crate::material::tokens::typography::TypeRole::BodyLarge,
        trailing_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
    }
}
