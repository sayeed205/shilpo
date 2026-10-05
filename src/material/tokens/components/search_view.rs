use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::foundation::ElevationRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::typography::TypeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_color: ColorRole,
    pub container_elevation: ElevationRole,
    pub divider_color: ColorRole,
    pub docked_container_shape: ShapeRole,
    pub docked_header_container_height: Dp,
    pub full_screen_container_shape: ShapeRole,
    pub full_screen_header_container_height: Dp,
    pub header_input_text_color: ColorRole,
    pub header_input_text_font: TypeRole,
    pub header_leading_icon_color: ColorRole,
    pub header_supporting_text_color: ColorRole,
    pub header_supporting_text_font: TypeRole,
    pub header_trailing_icon_color: ColorRole,
}

/// Returns authored token values for this component.
pub const fn search_view() -> Tokens {
    Tokens {
        container_color: crate::material::tokens::color_role::ColorRole::SurfaceContainerHigh,
        container_elevation: crate::material::tokens::foundation::ElevationRole::Level3,
        divider_color: crate::material::tokens::color_role::ColorRole::Outline,
        docked_container_shape: crate::material::tokens::shape::ShapeRole::CornerExtraLarge,
        docked_header_container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42600000)),
        full_screen_container_shape: crate::material::tokens::shape::ShapeRole::CornerNone,
        full_screen_header_container_height: crate::material::tokens::units::Dp(f32::from_bits(
            0x42900000,
        )),
        header_input_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        header_input_text_font: crate::material::tokens::typography::TypeRole::BodyLarge,
        header_leading_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        header_supporting_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        header_supporting_text_font: crate::material::tokens::typography::TypeRole::BodyLarge,
        header_trailing_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
    }
}
