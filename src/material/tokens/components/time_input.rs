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
    pub container_shape: ShapeRole,
    pub focus_indicator_color: ColorRole,
    pub headline_color: ColorRole,
    pub headline_font: TypeRole,
    pub period_selector_container_height: Dp,
    pub period_selector_container_shape: ShapeRole,
    pub period_selector_container_width: Dp,
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
    pub time_field_container_color: ColorRole,
    pub time_field_container_height: Dp,
    pub time_field_container_shape: ShapeRole,
    pub time_field_container_width: Dp,
    pub time_field_focus_container_color: ColorRole,
    pub time_field_focus_label_text_color: ColorRole,
    pub time_field_focus_outline_color: ColorRole,
    pub time_field_focus_outline_width: Dp,
    pub time_field_hover_label_text_color: ColorRole,
    pub time_field_label_text_color: ColorRole,
    pub time_field_label_text_font: TypeRole,
    pub time_field_separator_color: ColorRole,
    pub time_field_separator_font: TypeRole,
    pub time_field_supporting_text_color: ColorRole,
    pub time_field_supporting_text_font: TypeRole,
}

/// Returns authored token values for this component.
pub const fn time_input() -> Tokens {
    Tokens {
        container_color: crate::material::tokens::color_role::ColorRole::SurfaceContainerHigh,
        container_elevation: crate::material::tokens::foundation::ElevationRole::Level3,
        container_shape: crate::material::tokens::shape::ShapeRole::CornerExtraLarge,
        focus_indicator_color: crate::material::tokens::color_role::ColorRole::Secondary,
        headline_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        headline_font: crate::material::tokens::typography::TypeRole::LabelMedium,
        period_selector_container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42900000)),
        period_selector_container_shape: crate::material::tokens::shape::ShapeRole::CornerSmall,
        period_selector_container_width: crate::material::tokens::units::Dp(f32::from_bits(0x42500000)),
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
        time_field_container_color:
            crate::material::tokens::color_role::ColorRole::SurfaceContainerHighest,
        time_field_container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42900000)),
        time_field_container_shape: crate::material::tokens::shape::ShapeRole::CornerSmall,
        time_field_container_width: crate::material::tokens::units::Dp(f32::from_bits(0x42C00000)),
        time_field_focus_container_color: crate::material::tokens::color_role::ColorRole::PrimaryContainer,
        time_field_focus_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnPrimaryContainer,
        time_field_focus_outline_color: crate::material::tokens::color_role::ColorRole::Primary,
        time_field_focus_outline_width: crate::material::tokens::units::Dp(f32::from_bits(0x40000000)),
        time_field_hover_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        time_field_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        time_field_label_text_font: crate::material::tokens::typography::TypeRole::DisplayMedium,
        time_field_separator_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        time_field_separator_font: crate::material::tokens::typography::TypeRole::DisplayLarge,
        time_field_supporting_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        time_field_supporting_text_font: crate::material::tokens::typography::TypeRole::BodySmall,
    }
}
