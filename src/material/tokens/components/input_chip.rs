use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::foundation::ElevationRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::typography::TypeRole;
use crate::material::tokens::units::{Dp, Opacity};

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_elevation: ElevationRole,
    pub container_height: Dp,
    pub container_shape: ShapeRole,
    pub disabled_label_text_color: ColorRole,
    pub disabled_label_text_opacity: Opacity,
    pub disabled_selected_container_color: ColorRole,
    pub disabled_selected_container_opacity: Opacity,
    pub disabled_unselected_outline_color: ColorRole,
    pub disabled_unselected_outline_opacity: Opacity,
    pub dragged_container_elevation: ElevationRole,
    pub focus_indicator_color: ColorRole,
    pub label_text_font: TypeRole,
    pub selected_container_color: ColorRole,
    pub selected_dragged_label_text_color: ColorRole,
    pub selected_focus_label_text_color: ColorRole,
    pub selected_hover_label_text_color: ColorRole,
    pub selected_label_text_color: ColorRole,
    pub selected_outline_width: Dp,
    pub selected_pressed_label_text_color: ColorRole,
    pub unselected_dragged_label_text_color: ColorRole,
    pub unselected_focus_label_text_color: ColorRole,
    pub unselected_focus_outline_color: ColorRole,
    pub unselected_hover_label_text_color: ColorRole,
    pub unselected_label_text_color: ColorRole,
    pub unselected_outline_color: ColorRole,
    pub unselected_outline_width: Dp,
    pub unselected_pressed_label_text_color: ColorRole,
    pub avatar_shape: ShapeRole,
    pub avatar_size: Dp,
    pub disabled_avatar_opacity: Opacity,
    pub disabled_leading_icon_color: ColorRole,
    pub disabled_leading_icon_opacity: Opacity,
    pub leading_icon_size: Dp,
    pub selected_dragged_leading_icon_color: ColorRole,
    pub selected_focus_leading_icon_color: ColorRole,
    pub selected_hover_leading_icon_color: ColorRole,
    pub selected_leading_icon_color: ColorRole,
    pub selected_pressed_leading_icon_color: ColorRole,
    pub unselected_dragged_leading_icon_color: ColorRole,
    pub unselected_focus_leading_icon_color: ColorRole,
    pub unselected_hover_leading_icon_color: ColorRole,
    pub unselected_leading_icon_color: ColorRole,
    pub unselected_pressed_leading_icon_color: ColorRole,
    pub disabled_trailing_icon_color: ColorRole,
    pub disabled_trailing_icon_opacity: Opacity,
    pub selected_dragged_trailing_icon_color: ColorRole,
    pub selected_focus_trailing_icon_color: ColorRole,
    pub selected_hover_trailing_icon_color: ColorRole,
    pub selected_pressed_trailing_icon_color: ColorRole,
    pub selected_trailing_icon_color: ColorRole,
    pub trailing_icon_size: Dp,
    pub unselected_dragged_trailing_icon_color: ColorRole,
    pub unselected_focus_trailing_icon_color: ColorRole,
    pub unselected_hover_trailing_icon_color: ColorRole,
    pub unselected_pressed_trailing_icon_color: ColorRole,
    pub unselected_trailing_icon_color: ColorRole,
}

/// Returns authored token values for this component.
pub const fn input_chip() -> Tokens {
    Tokens {
        container_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42000000)),
        container_shape: crate::material::tokens::shape::ShapeRole::CornerSmall,
        disabled_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        disabled_label_text_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3EC28F5C)),
        disabled_selected_container_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        disabled_selected_container_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3DF5C28F,
        )),
        disabled_unselected_outline_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        disabled_unselected_outline_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3DF5C28F,
        )),
        dragged_container_elevation: crate::material::tokens::foundation::ElevationRole::Level4,
        focus_indicator_color: crate::material::tokens::color_role::ColorRole::Secondary,
        label_text_font: crate::material::tokens::typography::TypeRole::LabelLarge,
        selected_container_color: crate::material::tokens::color_role::ColorRole::SecondaryContainer,
        selected_dragged_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_focus_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_hover_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_label_text_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_outline_width: crate::material::tokens::units::Dp(f32::from_bits(0x00000000)),
        selected_pressed_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        unselected_dragged_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_focus_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_focus_outline_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_hover_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_outline_color: crate::material::tokens::color_role::ColorRole::OutlineVariant,
        unselected_outline_width: crate::material::tokens::units::Dp(f32::from_bits(0x3F800000)),
        unselected_pressed_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        avatar_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        avatar_size: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
        disabled_avatar_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3EC28F5C)),
        disabled_leading_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        disabled_leading_icon_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3EC28F5C)),
        leading_icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41900000)),
        selected_dragged_leading_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_focus_leading_icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        selected_hover_leading_icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        selected_leading_icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        selected_pressed_leading_icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        unselected_dragged_leading_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_focus_leading_icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        unselected_hover_leading_icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        unselected_leading_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_pressed_leading_icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        disabled_trailing_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        disabled_trailing_icon_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3EC28F5C,
        )),
        selected_dragged_trailing_icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        selected_focus_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_hover_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_pressed_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_trailing_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        trailing_icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41900000)),
        unselected_dragged_trailing_icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        unselected_focus_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_hover_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_pressed_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_trailing_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
    }
}
