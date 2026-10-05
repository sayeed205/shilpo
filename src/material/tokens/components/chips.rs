use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::foundation::ElevationRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::typography::TypeRole;
use crate::material::tokens::units::{Dp, Opacity};

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub avatar_shape: ShapeRole,
    pub avatar_size: Dp,
    pub container_elevation: ElevationRole,
    pub disabled_label_text_color: ColorRole,
    pub disabled_leading_icon_color: ColorRole,
    pub disabled_trailing_icon_color: ColorRole,
    pub dragged_container_elevation: ElevationRole,
    pub focused_indicator_color: ColorRole,
    pub height: Dp,
    pub label_text: TypeRole,
    pub leading_icon_size: Dp,
    pub pressed_shape: ShapeRole,
    pub selected_container_color: ColorRole,
    pub selected_disabled_container_color: ColorRole,
    pub selected_disabled_container_opacity: Opacity,
    pub selected_label_text_color: ColorRole,
    pub selected_leading_icon_color: ColorRole,
    pub selected_outline_width: Dp,
    pub selected_shape: ShapeRole,
    pub selected_trailing_icon_color: ColorRole,
    pub trailing_icon_size: Dp,
    pub unselected_disabled_outline_color: ColorRole,
    pub unselected_disabled_outline_opacity: Opacity,
    pub unselected_label_text_color: ColorRole,
    pub unselected_leading_icon_color: ColorRole,
    pub unselected_outline_color: ColorRole,
    pub unselected_outline_width: Dp,
    pub unselected_shape: ShapeRole,
    pub unselected_trailing_icon_color: ColorRole,
}

/// Returns authored token values for this component.
pub const fn chips() -> Tokens {
    Tokens {
        avatar_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        avatar_size: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
        container_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        disabled_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        disabled_leading_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        disabled_trailing_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        dragged_container_elevation: crate::material::tokens::foundation::ElevationRole::Level4,
        focused_indicator_color: crate::material::tokens::color_role::ColorRole::Secondary,
        height: crate::material::tokens::units::Dp(f32::from_bits(0x42000000)),
        label_text: crate::material::tokens::typography::TypeRole::LabelLarge,
        leading_icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41900000)),
        pressed_shape: crate::material::tokens::shape::ShapeRole::CornerSmall,
        selected_container_color: crate::material::tokens::color_role::ColorRole::SecondaryContainer,
        selected_disabled_container_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        selected_disabled_container_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3DF5C28F,
        )),
        selected_label_text_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_leading_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_outline_width: crate::material::tokens::units::Dp(f32::from_bits(0x00000000)),
        selected_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        selected_trailing_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        trailing_icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41900000)),
        unselected_disabled_outline_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        unselected_disabled_outline_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3DCCCCCD,
        )),
        unselected_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_leading_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_outline_color: crate::material::tokens::color_role::ColorRole::OutlineVariant,
        unselected_outline_width: crate::material::tokens::units::Dp(f32::from_bits(0x3F800000)),
        unselected_shape: crate::material::tokens::shape::ShapeRole::CornerMedium,
        unselected_trailing_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
    }
}
