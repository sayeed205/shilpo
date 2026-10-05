use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::foundation::ElevationRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::typography::TypeRole;
use crate::material::tokens::units::{Dp, Opacity};

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_height: Dp,
    pub container_shape: ShapeRole,
    pub disabled_label_text_color: ColorRole,
    pub disabled_label_text_opacity: Opacity,
    pub dragged_container_elevation: ElevationRole,
    pub dragged_label_text_color: ColorRole,
    pub elevated_container_color: ColorRole,
    pub elevated_container_elevation: ElevationRole,
    pub elevated_disabled_container_color: ColorRole,
    pub elevated_disabled_container_elevation: ElevationRole,
    pub elevated_disabled_container_opacity: Opacity,
    pub elevated_focus_container_elevation: ElevationRole,
    pub elevated_hover_container_elevation: ElevationRole,
    pub elevated_pressed_container_elevation: ElevationRole,
    pub flat_container_elevation: ElevationRole,
    pub flat_disabled_outline_color: ColorRole,
    pub flat_disabled_outline_opacity: Opacity,
    pub flat_focus_outline_color: ColorRole,
    pub flat_outline_color: ColorRole,
    pub flat_outline_width: Dp,
    pub focus_indicator_color: ColorRole,
    pub focus_label_text_color: ColorRole,
    pub hover_label_text_color: ColorRole,
    pub label_text_color: ColorRole,
    pub label_text_font: TypeRole,
    pub pressed_label_text_color: ColorRole,
    pub disabled_icon_color: ColorRole,
    pub disabled_icon_opacity: Opacity,
    pub dragged_icon_color: ColorRole,
    pub focus_icon_color: ColorRole,
    pub hover_icon_color: ColorRole,
    pub icon_color: ColorRole,
    pub icon_size: Dp,
    pub pressed_icon_color: ColorRole,
}

/// Returns authored token values for this component.
pub const fn assist_chip() -> Tokens {
    Tokens {
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42000000)),
        container_shape: crate::material::tokens::shape::ShapeRole::CornerSmall,
        disabled_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        disabled_label_text_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3EC28F5C)),
        dragged_container_elevation: crate::material::tokens::foundation::ElevationRole::Level4,
        dragged_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        elevated_container_color: crate::material::tokens::color_role::ColorRole::SurfaceContainerLow,
        elevated_container_elevation: crate::material::tokens::foundation::ElevationRole::Level1,
        elevated_disabled_container_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        elevated_disabled_container_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        elevated_disabled_container_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3DF5C28F,
        )),
        elevated_focus_container_elevation: crate::material::tokens::foundation::ElevationRole::Level1,
        elevated_hover_container_elevation: crate::material::tokens::foundation::ElevationRole::Level2,
        elevated_pressed_container_elevation: crate::material::tokens::foundation::ElevationRole::Level1,
        flat_container_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        flat_disabled_outline_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        flat_disabled_outline_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3DF5C28F)),
        flat_focus_outline_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        flat_outline_color: crate::material::tokens::color_role::ColorRole::OutlineVariant,
        flat_outline_width: crate::material::tokens::units::Dp(f32::from_bits(0x3F800000)),
        focus_indicator_color: crate::material::tokens::color_role::ColorRole::Secondary,
        focus_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        hover_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        label_text_font: crate::material::tokens::typography::TypeRole::LabelLarge,
        pressed_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        disabled_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        disabled_icon_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3EC28F5C)),
        dragged_icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        focus_icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        hover_icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41900000)),
        pressed_icon_color: crate::material::tokens::color_role::ColorRole::Primary,
    }
}
