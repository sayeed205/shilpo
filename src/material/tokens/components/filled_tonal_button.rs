use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::foundation::ElevationRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::typography::TypeRole;
use crate::material::tokens::units::{Dp, Opacity};

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_color: ColorRole,
    pub container_elevation: ElevationRole,
    pub container_height: Dp,
    pub container_shape: ShapeRole,
    pub disabled_container_color: ColorRole,
    pub disabled_container_elevation: ElevationRole,
    pub disabled_container_opacity: Opacity,
    pub disabled_label_text_color: ColorRole,
    pub disabled_label_text_opacity: Opacity,
    pub focus_container_elevation: ElevationRole,
    pub focus_label_text_color: ColorRole,
    pub hover_container_elevation: ElevationRole,
    pub hover_label_text_color: ColorRole,
    pub label_text_color: ColorRole,
    pub label_text_font: TypeRole,
    pub pressed_container_elevation: ElevationRole,
    pub pressed_label_text_color: ColorRole,
    pub disabled_icon_color: ColorRole,
    pub disabled_icon_opacity: Opacity,
    pub focus_icon_color: ColorRole,
    pub hover_icon_color: ColorRole,
    pub icon_color: ColorRole,
    pub icon_size: Dp,
    pub pressed_icon_color: ColorRole,
}

/// Returns authored token values for this component.
pub const fn filled_tonal_button() -> Tokens {
    Tokens {
        container_color: crate::material::tokens::color_role::ColorRole::SecondaryContainer,
        container_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42200000)),
        container_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        disabled_container_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        disabled_container_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        disabled_container_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3DF5C28F)),
        disabled_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        disabled_label_text_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3EC28F5C)),
        focus_container_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        focus_label_text_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        hover_container_elevation: crate::material::tokens::foundation::ElevationRole::Level1,
        hover_label_text_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        label_text_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        label_text_font: crate::material::tokens::typography::TypeRole::LabelLarge,
        pressed_container_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        pressed_label_text_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        disabled_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        disabled_icon_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3EC28F5C)),
        focus_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        hover_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41900000)),
        pressed_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
    }
}
