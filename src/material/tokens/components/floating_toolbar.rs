use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_between_space: Dp,
    pub container_external_padding: Dp,
    pub container_height: Dp,
    pub container_leading_space: Dp,
    pub container_shape: ShapeRole,
    pub container_trailing_space: Dp,
    pub standard_container_color: ColorRole,
    pub vibrant_button_selected_container_color: ColorRole,
    pub vibrant_button_selected_icon_color: ColorRole,
    pub vibrant_button_selected_text_color: ColorRole,
    pub vibrant_button_unselected_icon_color: ColorRole,
    pub vibrant_button_unselected_text_color: ColorRole,
    pub vibrant_container_color: ColorRole,
}

/// Returns authored token values for this component.
pub const fn floating_toolbar() -> Tokens {
    Tokens {
        container_between_space: crate::material::tokens::units::Dp(f32::from_bits(0x40800000)),
        container_external_padding: crate::material::tokens::units::Dp(f32::from_bits(0x41800000)),
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42800000)),
        container_leading_space: crate::material::tokens::units::Dp(f32::from_bits(0x41000000)),
        container_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        container_trailing_space: crate::material::tokens::units::Dp(f32::from_bits(0x41000000)),
        standard_container_color: crate::material::tokens::color_role::ColorRole::SurfaceContainer,
        vibrant_button_selected_container_color:
            crate::material::tokens::color_role::ColorRole::SurfaceContainer,
        vibrant_button_selected_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        vibrant_button_selected_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        vibrant_button_unselected_icon_color:
            crate::material::tokens::color_role::ColorRole::OnPrimaryContainer,
        vibrant_button_unselected_text_color:
            crate::material::tokens::color_role::ColorRole::OnPrimaryContainer,
        vibrant_container_color: crate::material::tokens::color_role::ColorRole::PrimaryContainer,
    }
}
