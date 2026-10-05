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
    pub container_width: Dp,
    pub date_container_height: Dp,
    pub date_container_shape: ShapeRole,
    pub date_container_width: Dp,
    pub date_label_text_font: TypeRole,
    pub date_selected_container_color: ColorRole,
    pub date_selected_label_text_color: ColorRole,
    pub date_state_layer_height: Dp,
    pub date_state_layer_shape: ShapeRole,
    pub date_state_layer_width: Dp,
    pub date_today_container_outline_color: ColorRole,
    pub date_today_container_outline_width: Dp,
    pub date_today_label_text_color: ColorRole,
    pub date_unselected_label_text_color: ColorRole,
    pub header_container_height: Dp,
    pub header_container_width: Dp,
    pub header_headline_color: ColorRole,
    pub header_headline_font: TypeRole,
    pub header_supporting_text_color: ColorRole,
    pub header_supporting_text_font: TypeRole,
    pub range_selection_active_indicator_container_color: ColorRole,
    pub range_selection_active_indicator_container_height: Dp,
    pub range_selection_active_indicator_container_shape: ShapeRole,
    pub range_selection_container_elevation: ElevationRole,
    pub range_selection_container_shape: ShapeRole,
    pub selection_date_in_range_label_text_color: ColorRole,
    pub range_selection_header_container_height: Dp,
    pub range_selection_header_headline_font: TypeRole,
    pub range_selection_month_subhead_color: ColorRole,
    pub range_selection_month_subhead_font: TypeRole,
    pub weekdays_label_text_color: ColorRole,
    pub weekdays_label_text_font: TypeRole,
    pub selection_year_container_height: Dp,
    pub selection_year_container_width: Dp,
    pub selection_year_label_text_font: TypeRole,
    pub selection_year_selected_container_color: ColorRole,
    pub selection_year_selected_label_text_color: ColorRole,
    pub selection_year_state_layer_height: Dp,
    pub selection_year_state_layer_shape: ShapeRole,
    pub selection_year_state_layer_width: Dp,
    pub selection_year_unselected_label_text_color: ColorRole,
}

/// Returns authored token values for this component.
pub const fn date_picker_modal() -> Tokens {
    Tokens {
        container_color: crate::material::tokens::color_role::ColorRole::SurfaceContainerHigh,
        container_elevation: crate::material::tokens::foundation::ElevationRole::Level3,
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x440E0000)),
        container_shape: crate::material::tokens::shape::ShapeRole::CornerExtraLarge,
        container_width: crate::material::tokens::units::Dp(f32::from_bits(0x43B40000)),
        date_container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42200000)),
        date_container_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        date_container_width: crate::material::tokens::units::Dp(f32::from_bits(0x42200000)),
        date_label_text_font: crate::material::tokens::typography::TypeRole::BodyLarge,
        date_selected_container_color: crate::material::tokens::color_role::ColorRole::Primary,
        date_selected_label_text_color: crate::material::tokens::color_role::ColorRole::OnPrimary,
        date_state_layer_height: crate::material::tokens::units::Dp(f32::from_bits(0x42200000)),
        date_state_layer_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        date_state_layer_width: crate::material::tokens::units::Dp(f32::from_bits(0x42200000)),
        date_today_container_outline_color: crate::material::tokens::color_role::ColorRole::Primary,
        date_today_container_outline_width: crate::material::tokens::units::Dp(f32::from_bits(0x3F800000)),
        date_today_label_text_color: crate::material::tokens::color_role::ColorRole::Primary,
        date_unselected_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        header_container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42F00000)),
        header_container_width: crate::material::tokens::units::Dp(f32::from_bits(0x43B40000)),
        header_headline_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        header_headline_font: crate::material::tokens::typography::TypeRole::HeadlineLarge,
        header_supporting_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        header_supporting_text_font: crate::material::tokens::typography::TypeRole::LabelLarge,
        range_selection_active_indicator_container_color:
            crate::material::tokens::color_role::ColorRole::SecondaryContainer,
        range_selection_active_indicator_container_height: crate::material::tokens::units::Dp(
            f32::from_bits(0x42200000),
        ),
        range_selection_active_indicator_container_shape:
            crate::material::tokens::shape::ShapeRole::CornerFull,
        range_selection_container_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        range_selection_container_shape: crate::material::tokens::shape::ShapeRole::CornerNone,
        selection_date_in_range_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        range_selection_header_container_height: crate::material::tokens::units::Dp(f32::from_bits(
            0x43000000,
        )),
        range_selection_header_headline_font: crate::material::tokens::typography::TypeRole::TitleLarge,
        range_selection_month_subhead_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        range_selection_month_subhead_font: crate::material::tokens::typography::TypeRole::TitleSmall,
        weekdays_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        weekdays_label_text_font: crate::material::tokens::typography::TypeRole::BodyLarge,
        selection_year_container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42100000)),
        selection_year_container_width: crate::material::tokens::units::Dp(f32::from_bits(0x42900000)),
        selection_year_label_text_font: crate::material::tokens::typography::TypeRole::BodyLarge,
        selection_year_selected_container_color: crate::material::tokens::color_role::ColorRole::Primary,
        selection_year_selected_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnPrimary,
        selection_year_state_layer_height: crate::material::tokens::units::Dp(f32::from_bits(0x42100000)),
        selection_year_state_layer_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        selection_year_state_layer_width: crate::material::tokens::units::Dp(f32::from_bits(0x42900000)),
        selection_year_unselected_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
    }
}
