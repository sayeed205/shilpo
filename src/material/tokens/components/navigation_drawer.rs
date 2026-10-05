use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::foundation::ElevationRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::typography::TypeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub active_focus_icon_color: ColorRole,
    pub active_focus_label_text_color: ColorRole,
    pub active_hover_icon_color: ColorRole,
    pub active_hover_label_text_color: ColorRole,
    pub active_icon_color: ColorRole,
    pub active_indicator_color: ColorRole,
    pub active_indicator_height: Dp,
    pub active_indicator_shape: ShapeRole,
    pub active_indicator_width: Dp,
    pub active_label_text_color: ColorRole,
    pub active_pressed_icon_color: ColorRole,
    pub active_pressed_label_text_color: ColorRole,
    pub bottom_container_shape: ShapeRole,
    pub container_height_percent: f32,
    pub container_shape: ShapeRole,
    pub container_width: Dp,
    pub focus_indicator_color: ColorRole,
    pub headline_color: ColorRole,
    pub headline_font: TypeRole,
    pub icon_size: Dp,
    pub inactive_focus_icon_color: ColorRole,
    pub inactive_focus_label_text_color: ColorRole,
    pub inactive_hover_icon_color: ColorRole,
    pub inactive_hover_label_text_color: ColorRole,
    pub inactive_icon_color: ColorRole,
    pub inactive_label_text_color: ColorRole,
    pub inactive_pressed_icon_color: ColorRole,
    pub inactive_pressed_label_text_color: ColorRole,
    pub label_text_font: TypeRole,
    pub large_badge_label_color: ColorRole,
    pub large_badge_label_font: TypeRole,
    pub modal_container_color: ColorRole,
    pub modal_container_elevation: ElevationRole,
    pub standard_container_color: ColorRole,
    pub standard_container_elevation: ElevationRole,
}

/// Returns authored token values for this component.
pub const fn navigation_drawer() -> Tokens {
    Tokens {
        active_focus_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        active_focus_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        active_hover_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        active_hover_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        active_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        active_indicator_color: crate::material::tokens::color_role::ColorRole::SecondaryContainer,
        active_indicator_height: crate::material::tokens::units::Dp(f32::from_bits(0x42600000)),
        active_indicator_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        active_indicator_width: crate::material::tokens::units::Dp(f32::from_bits(0x43A80000)),
        active_label_text_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        active_pressed_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        active_pressed_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        bottom_container_shape: crate::material::tokens::shape::ShapeRole::CornerLargeTop,
        container_height_percent: f32::from_bits(0x42C80000),
        container_shape: crate::material::tokens::shape::ShapeRole::CornerLargeEnd,
        container_width: crate::material::tokens::units::Dp(f32::from_bits(0x43B40000)),
        focus_indicator_color: crate::material::tokens::color_role::ColorRole::Secondary,
        headline_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        headline_font: crate::material::tokens::typography::TypeRole::TitleSmall,
        icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
        inactive_focus_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        inactive_focus_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        inactive_hover_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        inactive_hover_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        inactive_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        inactive_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        inactive_pressed_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        inactive_pressed_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        label_text_font: crate::material::tokens::typography::TypeRole::LabelLarge,
        large_badge_label_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        large_badge_label_font: crate::material::tokens::typography::TypeRole::LabelLarge,
        modal_container_color: crate::material::tokens::color_role::ColorRole::SurfaceContainerLow,
        modal_container_elevation: crate::material::tokens::foundation::ElevationRole::Level1,
        standard_container_color: crate::material::tokens::color_role::ColorRole::Surface,
        standard_container_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
    }
}
