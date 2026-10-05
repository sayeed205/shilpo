use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::units::Opacity;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub disabled_container_color: ColorRole,
    pub disabled_container_opacity: Opacity,
    pub disabled_icon_color: ColorRole,
    pub disabled_icon_opacity: Opacity,
    pub disabled_label_color: ColorRole,
    pub disabled_label_opacity: Opacity,
    pub focused_icon_color: ColorRole,
    pub focused_label_color: ColorRole,
    pub hovered_icon_color: ColorRole,
    pub hovered_label_color: ColorRole,
    pub icon_color: ColorRole,
    pub label_color: ColorRole,
    pub pressed_icon_color: ColorRole,
    pub pressed_label_color: ColorRole,
}

/// Returns authored token values for this component.
pub const fn text_button() -> Tokens {
    Tokens {
        disabled_container_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        disabled_container_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3DCCCCCD)),
        disabled_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        disabled_icon_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3EC28F5C)),
        disabled_label_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        disabled_label_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3EC28F5C)),
        focused_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        focused_label_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        hovered_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        hovered_label_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        label_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        pressed_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        pressed_label_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
    }
}
