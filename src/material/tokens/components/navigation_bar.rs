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
    pub item_active_icon_color: ColorRole,
    pub item_active_indicator_color: ColorRole,
    pub item_active_indicator_icon_label_space: Dp,
    pub item_active_indicator_shape: ShapeRole,
    pub item_active_label_text_color: ColorRole,
    pub item_between_space: Dp,
    pub item_inactive_icon_color: ColorRole,
    pub item_inactive_label_text_color: ColorRole,
    pub nav_shape: ShapeRole,
    pub tall_container_height: Dp,
    pub label_text_font: TypeRole,
}

/// Returns authored token values for this component.
pub const fn navigation_bar() -> Tokens {
    Tokens {
        container_color: crate::material::tokens::color_role::ColorRole::SurfaceContainer,
        container_elevation: crate::material::tokens::foundation::ElevationRole::Level2,
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42800000)),
        item_active_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        item_active_indicator_color: crate::material::tokens::color_role::ColorRole::SecondaryContainer,
        item_active_indicator_icon_label_space: crate::material::tokens::units::Dp(f32::from_bits(
            0x40800000,
        )),
        item_active_indicator_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        item_active_label_text_color: crate::material::tokens::color_role::ColorRole::Secondary,
        item_between_space: crate::material::tokens::units::Dp(f32::from_bits(0x00000000)),
        item_inactive_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        item_inactive_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        nav_shape: crate::material::tokens::shape::ShapeRole::CornerNone,
        tall_container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42A00000)),
        label_text_font: crate::material::tokens::typography::TypeRole::LabelMedium,
    }
}
