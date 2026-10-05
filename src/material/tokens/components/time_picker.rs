use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::foundation::ElevationRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::typography::TypeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub clock_dial_color: ColorRole,
    pub clock_dial_container_size: Dp,
    pub clock_dial_label_text_font: TypeRole,
    pub clock_dial_selected_label_text_color: ColorRole,
    pub clock_dial_selector_center_container_color: ColorRole,
    pub clock_dial_selector_center_container_shape: ShapeRole,
    pub clock_dial_selector_center_container_size: Dp,
    pub clock_dial_selector_handle_container_color: ColorRole,
    pub clock_dial_selector_handle_container_shape: ShapeRole,
    pub clock_dial_selector_handle_container_size: Dp,
    pub clock_dial_selector_track_container_color: ColorRole,
    pub clock_dial_selector_track_container_width: Dp,
    pub clock_dial_shape: ShapeRole,
    pub clock_dial_unselected_label_text_color: ColorRole,
    pub container_color: ColorRole,
    pub container_elevation: ElevationRole,
    pub container_shape: ShapeRole,
    pub headline_color: ColorRole,
    pub headline_font: TypeRole,
    pub period_selector_container_shape: ShapeRole,
    pub period_selector_horizontal_container_height: Dp,
    pub period_selector_horizontal_container_width: Dp,
    pub period_selector_label_text_font: TypeRole,
    pub period_selector_outline_color: ColorRole,
    pub period_selector_outline_width: Dp,
    pub period_selector_selected_container_color: ColorRole,
    pub period_selector_selected_focus_label_text_color: ColorRole,
    pub period_selector_selected_hover_label_text_color: ColorRole,
    pub period_selector_selected_label_text_color: ColorRole,
    pub period_selector_selected_pressed_label_text_color: ColorRole,
    pub period_selector_unselected_focus_label_text_color: ColorRole,
    pub period_selector_unselected_hover_label_text_color: ColorRole,
    pub period_selector_unselected_label_text_color: ColorRole,
    pub period_selector_unselected_pressed_label_text_color: ColorRole,
    pub period_selector_vertical_container_height: Dp,
    pub period_selector_vertical_container_width: Dp,
    pub time_selector24_h_vertical_container_width: Dp,
    pub time_selector_container_height: Dp,
    pub time_selector_container_shape: ShapeRole,
    pub time_selector_container_width: Dp,
    pub time_selector_label_text_font: TypeRole,
    pub time_selector_selected_container_color: ColorRole,
    pub time_selector_selected_focus_label_text_color: ColorRole,
    pub time_selector_selected_hover_label_text_color: ColorRole,
    pub time_selector_selected_label_text_color: ColorRole,
    pub time_selector_selected_pressed_label_text_color: ColorRole,
    pub time_selector_separator_color: ColorRole,
    pub time_selector_separator_font: TypeRole,
    pub time_selector_unselected_container_color: ColorRole,
    pub time_selector_unselected_focus_label_text_color: ColorRole,
    pub time_selector_unselected_hover_label_text_color: ColorRole,
    pub time_selector_unselected_label_text_color: ColorRole,
    pub time_selector_unselected_pressed_label_text_color: ColorRole,
}

/// Returns authored token values for this component.
pub const fn time_picker() -> Tokens {
    Tokens {
        clock_dial_color: crate::material::tokens::color_role::ColorRole::SurfaceContainerHighest,
        clock_dial_container_size: crate::material::tokens::units::Dp(f32::from_bits(0x43800000)),
        clock_dial_label_text_font: crate::material::tokens::typography::TypeRole::BodyLarge,
        clock_dial_selected_label_text_color: crate::material::tokens::color_role::ColorRole::OnPrimary,
        clock_dial_selector_center_container_color:
            crate::material::tokens::color_role::ColorRole::Primary,
        clock_dial_selector_center_container_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        clock_dial_selector_center_container_size: crate::material::tokens::units::Dp(f32::from_bits(
            0x41000000,
        )),
        clock_dial_selector_handle_container_color:
            crate::material::tokens::color_role::ColorRole::Primary,
        clock_dial_selector_handle_container_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        clock_dial_selector_handle_container_size: crate::material::tokens::units::Dp(f32::from_bits(
            0x42400000,
        )),
        clock_dial_selector_track_container_color: crate::material::tokens::color_role::ColorRole::Primary,
        clock_dial_selector_track_container_width: crate::material::tokens::units::Dp(f32::from_bits(
            0x40000000,
        )),
        clock_dial_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        clock_dial_unselected_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        container_color: crate::material::tokens::color_role::ColorRole::SurfaceContainerHigh,
        container_elevation: crate::material::tokens::foundation::ElevationRole::Level3,
        container_shape: crate::material::tokens::shape::ShapeRole::CornerExtraLarge,
        headline_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        headline_font: crate::material::tokens::typography::TypeRole::LabelMedium,
        period_selector_container_shape: crate::material::tokens::shape::ShapeRole::CornerSmall,
        period_selector_horizontal_container_height: crate::material::tokens::units::Dp(f32::from_bits(
            0x42180000,
        )),
        period_selector_horizontal_container_width: crate::material::tokens::units::Dp(f32::from_bits(
            0x43580000,
        )),
        period_selector_label_text_font: crate::material::tokens::typography::TypeRole::TitleMedium,
        period_selector_outline_color: crate::material::tokens::color_role::ColorRole::Outline,
        period_selector_outline_width: crate::material::tokens::units::Dp(f32::from_bits(0x3F800000)),
        period_selector_selected_container_color:
            crate::material::tokens::color_role::ColorRole::TertiaryContainer,
        period_selector_selected_focus_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        period_selector_selected_hover_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        period_selector_selected_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        period_selector_selected_pressed_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        period_selector_unselected_focus_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        period_selector_unselected_hover_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        period_selector_unselected_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        period_selector_unselected_pressed_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        period_selector_vertical_container_height: crate::material::tokens::units::Dp(f32::from_bits(
            0x42A00000,
        )),
        period_selector_vertical_container_width: crate::material::tokens::units::Dp(f32::from_bits(
            0x42500000,
        )),
        time_selector24_h_vertical_container_width: crate::material::tokens::units::Dp(f32::from_bits(
            0x42E40000,
        )),
        time_selector_container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42A00000)),
        time_selector_container_shape: crate::material::tokens::shape::ShapeRole::CornerSmall,
        time_selector_container_width: crate::material::tokens::units::Dp(f32::from_bits(0x42C00000)),
        time_selector_label_text_font: crate::material::tokens::typography::TypeRole::DisplayLarge,
        time_selector_selected_container_color:
            crate::material::tokens::color_role::ColorRole::PrimaryContainer,
        time_selector_selected_focus_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnPrimaryContainer,
        time_selector_selected_hover_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnPrimaryContainer,
        time_selector_selected_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnPrimaryContainer,
        time_selector_selected_pressed_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnPrimaryContainer,
        time_selector_separator_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        time_selector_separator_font: crate::material::tokens::typography::TypeRole::DisplayLarge,
        time_selector_unselected_container_color:
            crate::material::tokens::color_role::ColorRole::SurfaceContainerHighest,
        time_selector_unselected_focus_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurface,
        time_selector_unselected_hover_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurface,
        time_selector_unselected_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurface,
        time_selector_unselected_pressed_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurface,
    }
}
