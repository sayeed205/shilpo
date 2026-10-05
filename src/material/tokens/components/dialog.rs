use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::foundation::ElevationRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::typography::TypeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub action_focus_label_text_color: ColorRole,
    pub action_hover_label_text_color: ColorRole,
    pub action_label_text_color: ColorRole,
    pub action_label_text_font: TypeRole,
    pub action_pressed_label_text_color: ColorRole,
    pub container_color: ColorRole,
    pub container_elevation: ElevationRole,
    pub container_shape: ShapeRole,
    pub headline_color: ColorRole,
    pub headline_font: TypeRole,
    pub supporting_text_color: ColorRole,
    pub supporting_text_font: TypeRole,
    pub icon_color: ColorRole,
    pub icon_size: Dp,
}

/// Returns authored token values for this component.
pub const fn dialog() -> Tokens {
    Tokens {
        action_focus_label_text_color: crate::material::tokens::color_role::ColorRole::Primary,
        action_hover_label_text_color: crate::material::tokens::color_role::ColorRole::Primary,
        action_label_text_color: crate::material::tokens::color_role::ColorRole::Primary,
        action_label_text_font: crate::material::tokens::typography::TypeRole::LabelLarge,
        action_pressed_label_text_color: crate::material::tokens::color_role::ColorRole::Primary,
        container_color: crate::material::tokens::color_role::ColorRole::SurfaceContainerHigh,
        container_elevation: crate::material::tokens::foundation::ElevationRole::Level3,
        container_shape: crate::material::tokens::shape::ShapeRole::CornerExtraLarge,
        headline_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        headline_font: crate::material::tokens::typography::TypeRole::HeadlineSmall,
        supporting_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        supporting_text_font: crate::material::tokens::typography::TypeRole::BodyMedium,
        icon_color: crate::material::tokens::color_role::ColorRole::Secondary,
        icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
    }
}
