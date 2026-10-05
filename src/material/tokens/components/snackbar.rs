use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::foundation::ElevationRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::typography::TypeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub action_focus_label_text_color: ColorRole,
    pub action_hover_label_text_color: ColorRole,
    pub action_label_text_color: ColorRole,
    pub action_label_text_font: TypeRole,
    pub action_pressed_label_text_color: ColorRole,
    pub container_color: ColorRole,
    pub container_elevation: ElevationRole,
    pub container_shape: ShapeRole,
    pub icon_color: ColorRole,
    pub focus_icon_color: ColorRole,
    pub hover_icon_color: ColorRole,
    pub pressed_icon_color: ColorRole,
    pub icon_size: Dp,
    pub supporting_text_color: ColorRole,
    pub supporting_text_font: TypeRole,
    pub single_line_container_height: Dp,
    pub two_lines_container_height: Dp,
}

/// Returns authored token values for this component.
pub const fn snackbar() -> Tokens {
    Tokens {
        action_focus_label_text_color: crate::material::tokens::color_role::ColorRole::InversePrimary,
        action_hover_label_text_color: crate::material::tokens::color_role::ColorRole::InversePrimary,
        action_label_text_color: crate::material::tokens::color_role::ColorRole::InversePrimary,
        action_label_text_font: crate::material::tokens::typography::TypeRole::LabelLarge,
        action_pressed_label_text_color: crate::material::tokens::color_role::ColorRole::InversePrimary,
        container_color: crate::material::tokens::color_role::ColorRole::InverseSurface,
        container_elevation: crate::material::tokens::foundation::ElevationRole::Level3,
        container_shape: crate::material::tokens::shape::ShapeRole::CornerExtraSmall,
        icon_color: crate::material::tokens::color_role::ColorRole::InverseOnSurface,
        focus_icon_color: crate::material::tokens::color_role::ColorRole::InverseOnSurface,
        hover_icon_color: crate::material::tokens::color_role::ColorRole::InverseOnSurface,
        pressed_icon_color: crate::material::tokens::color_role::ColorRole::InverseOnSurface,
        icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
        supporting_text_color: crate::material::tokens::color_role::ColorRole::InverseOnSurface,
        supporting_text_font: crate::material::tokens::typography::TypeRole::BodyMedium,
        single_line_container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42400000)),
        two_lines_container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42880000)),
    }
}
