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
    pub elevated_container_elevation: ElevationRole,
    pub elevated_disabled_container_color: ColorRole,
    pub elevated_disabled_container_elevation: ElevationRole,
    pub elevated_disabled_container_opacity: Opacity,
    pub elevated_focus_container_elevation: ElevationRole,
    pub elevated_hover_container_elevation: ElevationRole,
    pub elevated_pressed_container_elevation: ElevationRole,
    pub elevated_selected_container_color: ColorRole,
    pub elevated_unselected_container_color: ColorRole,
    pub flat_container_elevation: ElevationRole,
    pub flat_disabled_selected_container_color: ColorRole,
    pub flat_disabled_selected_container_opacity: Opacity,
    pub flat_disabled_unselected_outline_color: ColorRole,
    pub flat_disabled_unselected_outline_opacity: Opacity,
    pub flat_selected_container_color: ColorRole,
    pub flat_selected_focus_container_elevation: ElevationRole,
    pub flat_selected_hover_container_elevation: ElevationRole,
    pub flat_selected_outline_width: Dp,
    pub flat_selected_pressed_container_elevation: ElevationRole,
    pub flat_unselected_focus_container_elevation: ElevationRole,
    pub flat_unselected_focus_outline_color: ColorRole,
    pub flat_unselected_hover_container_elevation: ElevationRole,
    pub flat_unselected_outline_color: ColorRole,
    pub flat_unselected_outline_width: Dp,
    pub flat_unselected_pressed_container_elevation: ElevationRole,
    pub focus_indicator_color: ColorRole,
    pub label_text_font: TypeRole,
    pub selected_dragged_label_text_color: ColorRole,
    pub selected_focus_label_text_color: ColorRole,
    pub selected_hover_label_text_color: ColorRole,
    pub selected_label_text_color: ColorRole,
    pub selected_pressed_label_text_color: ColorRole,
    pub unselected_dragged_label_text_color: ColorRole,
    pub unselected_focus_label_text_color: ColorRole,
    pub unselected_hover_label_text_color: ColorRole,
    pub unselected_label_text_color: ColorRole,
    pub unselected_pressed_label_text_color: ColorRole,
    pub icon_size: Dp,
    pub disabled_leading_icon_color: ColorRole,
    pub disabled_leading_icon_opacity: Opacity,
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
    pub unselected_dragged_trailing_icon_color: ColorRole,
    pub unselected_focus_trailing_icon_color: ColorRole,
    pub unselected_hover_trailing_icon_color: ColorRole,
    pub unselected_pressed_trailing_icon_color: ColorRole,
    pub unselected_trailing_icon_color: ColorRole,
}

/// Returns authored token values for this component.
pub const fn filter_chip() -> Tokens {
    Tokens {
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42000000)),
        container_shape: crate::material::tokens::shape::ShapeRole::CornerSmall,
        disabled_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        disabled_label_text_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3EC28F5C)),
        dragged_container_elevation: crate::material::tokens::foundation::ElevationRole::Level4,
        elevated_container_elevation: crate::material::tokens::foundation::ElevationRole::Level1,
        elevated_disabled_container_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        elevated_disabled_container_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        elevated_disabled_container_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3DF5C28F,
        )),
        elevated_focus_container_elevation: crate::material::tokens::foundation::ElevationRole::Level1,
        elevated_hover_container_elevation: crate::material::tokens::foundation::ElevationRole::Level2,
        elevated_pressed_container_elevation: crate::material::tokens::foundation::ElevationRole::Level1,
        elevated_selected_container_color:
            crate::material::tokens::color_role::ColorRole::SecondaryContainer,
        elevated_unselected_container_color:
            crate::material::tokens::color_role::ColorRole::SurfaceContainerLow,
        flat_container_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        flat_disabled_selected_container_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        flat_disabled_selected_container_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3DF5C28F,
        )),
        flat_disabled_unselected_outline_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        flat_disabled_unselected_outline_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3DF5C28F,
        )),
        flat_selected_container_color: crate::material::tokens::color_role::ColorRole::SecondaryContainer,
        flat_selected_focus_container_elevation:
            crate::material::tokens::foundation::ElevationRole::Level0,
        flat_selected_hover_container_elevation:
            crate::material::tokens::foundation::ElevationRole::Level1,
        flat_selected_outline_width: crate::material::tokens::units::Dp(f32::from_bits(0x00000000)),
        flat_selected_pressed_container_elevation:
            crate::material::tokens::foundation::ElevationRole::Level0,
        flat_unselected_focus_container_elevation:
            crate::material::tokens::foundation::ElevationRole::Level0,
        flat_unselected_focus_outline_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        flat_unselected_hover_container_elevation:
            crate::material::tokens::foundation::ElevationRole::Level0,
        flat_unselected_outline_color: crate::material::tokens::color_role::ColorRole::OutlineVariant,
        flat_unselected_outline_width: crate::material::tokens::units::Dp(f32::from_bits(0x3F800000)),
        flat_unselected_pressed_container_elevation:
            crate::material::tokens::foundation::ElevationRole::Level0,
        focus_indicator_color: crate::material::tokens::color_role::ColorRole::Secondary,
        label_text_font: crate::material::tokens::typography::TypeRole::LabelLarge,
        selected_dragged_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_focus_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_hover_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_label_text_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_pressed_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        unselected_dragged_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_focus_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_hover_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_pressed_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41900000)),
        disabled_leading_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        disabled_leading_icon_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3EC28F5C)),
        selected_dragged_leading_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_focus_leading_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_hover_leading_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_leading_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_pressed_leading_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        unselected_dragged_leading_icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        unselected_focus_leading_icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        unselected_hover_leading_icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        unselected_leading_icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        unselected_pressed_leading_icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        disabled_trailing_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        disabled_trailing_icon_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3EC28F5C,
        )),
        selected_dragged_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_focus_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_hover_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_pressed_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_trailing_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        unselected_dragged_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_focus_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_hover_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_pressed_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_trailing_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
    }
}
