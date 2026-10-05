use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::foundation::ElevationRole;
use crate::material::tokens::shape::Shape;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::typography::TypeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub active_indicator_color: ColorRole,
    pub active_indicator_height: Dp,
    pub active_indicator_shape: Shape,
    pub container_color: ColorRole,
    pub container_elevation: ElevationRole,
    pub container_height: Dp,
    pub container_shape: ShapeRole,
    pub active_focus_icon_color: ColorRole,
    pub active_hover_icon_color: ColorRole,
    pub active_icon_color: ColorRole,
    pub active_pressed_icon_color: ColorRole,
    pub icon_and_label_text_container_height: Dp,
    pub icon_size: Dp,
    pub inactive_focus_icon_color: ColorRole,
    pub inactive_hover_icon_color: ColorRole,
    pub inactive_icon_color: ColorRole,
    pub inactive_pressed_icon_color: ColorRole,
    pub active_focus_label_text_color: ColorRole,
    pub active_hover_label_text_color: ColorRole,
    pub active_label_text_color: ColorRole,
    pub active_pressed_label_text_color: ColorRole,
    pub inactive_focus_label_text_color: ColorRole,
    pub inactive_hover_label_text_color: ColorRole,
    pub inactive_label_text_color: ColorRole,
    pub inactive_pressed_label_text_color: ColorRole,
    pub label_text_font: TypeRole,
}

/// Returns authored token values for this component.
pub const fn primary_navigation_tab() -> Tokens {
    Tokens {
        active_indicator_color: crate::material::tokens::color_role::ColorRole::Primary,
        active_indicator_height: crate::material::tokens::units::Dp(f32::from_bits(0x40400000)),
        active_indicator_shape: crate::material::tokens::shape::Shape::Corners(
            crate::material::tokens::shape::LogicalCorners {
                top_start: crate::material::tokens::shape::CornerSize::Dp(crate::material::tokens::units::Dp(
                    f32::from_bits(0x40400000),
                )),
                top_end: crate::material::tokens::shape::CornerSize::Dp(crate::material::tokens::units::Dp(
                    f32::from_bits(0x40400000),
                )),
                bottom_end: crate::material::tokens::shape::CornerSize::Dp(crate::material::tokens::units::Dp(
                    f32::from_bits(0x40400000),
                )),
                bottom_start: crate::material::tokens::shape::CornerSize::Dp(crate::material::tokens::units::Dp(
                    f32::from_bits(0x40400000),
                )),
            },
        ),
        container_color: crate::material::tokens::color_role::ColorRole::Surface,
        container_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42400000)),
        container_shape: crate::material::tokens::shape::ShapeRole::CornerNone,
        active_focus_icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        active_hover_icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        active_icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        active_pressed_icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        icon_and_label_text_container_height: crate::material::tokens::units::Dp(f32::from_bits(
            0x42800000,
        )),
        icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
        inactive_focus_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        inactive_hover_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        inactive_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        inactive_pressed_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        active_focus_label_text_color: crate::material::tokens::color_role::ColorRole::Primary,
        active_hover_label_text_color: crate::material::tokens::color_role::ColorRole::Primary,
        active_label_text_color: crate::material::tokens::color_role::ColorRole::Primary,
        active_pressed_label_text_color: crate::material::tokens::color_role::ColorRole::Primary,
        inactive_focus_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        inactive_hover_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        inactive_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        inactive_pressed_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        label_text_font: crate::material::tokens::typography::TypeRole::TitleSmall,
    }
}
