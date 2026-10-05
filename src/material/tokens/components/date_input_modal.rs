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
    pub container_height: Dp,
    pub container_shape: ShapeRole,
    pub container_surface_tint_layer_color: ColorRole,
    pub container_width: Dp,
    pub header_container_height: Dp,
    pub header_container_width: Dp,
    pub header_headline_color: ColorRole,
    pub header_headline_font: TypeRole,
    pub header_supporting_text_color: ColorRole,
    pub header_supporting_text_font: TypeRole,
}

/// Returns authored token values for this component.
pub const fn date_input_modal() -> Tokens {
    Tokens {
        container_color: crate::material::tokens::color_role::ColorRole::Surface,
        container_elevation: crate::material::tokens::foundation::ElevationRole::Level3,
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x44000000)),
        container_shape: crate::material::tokens::shape::ShapeRole::CornerExtraLarge,
        container_surface_tint_layer_color: crate::material::tokens::color_role::ColorRole::SurfaceTint,
        container_width: crate::material::tokens::units::Dp(f32::from_bits(0x43A40000)),
        header_container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42F00000)),
        header_container_width: crate::material::tokens::units::Dp(f32::from_bits(0x43A40000)),
        header_headline_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        header_headline_font: crate::material::tokens::typography::TypeRole::HeadlineLarge,
        header_supporting_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        header_supporting_text_font: crate::material::tokens::typography::TypeRole::LabelLarge,
    }
}
