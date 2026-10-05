use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::units::Opacity;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub disabled_color: ColorRole,
    pub disabled_opacity: Opacity,
    pub focused_color: ColorRole,
    pub hovered_color: ColorRole,
    pub color: ColorRole,
    pub pressed_color: ColorRole,
    pub selected_focused_color: ColorRole,
    pub selected_hovered_color: ColorRole,
    pub selected_color: ColorRole,
    pub selected_pressed_color: ColorRole,
    pub unselected_focused_color: ColorRole,
    pub unselected_hovered_color: ColorRole,
    pub unselected_color: ColorRole,
    pub unselected_pressed_color: ColorRole,
}

/// Returns authored token values for this component.
pub const fn standard_icon_button() -> Tokens {
    Tokens {
        disabled_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        disabled_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3EC28F5C)),
        focused_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        hovered_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        pressed_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        selected_focused_color: crate::material::tokens::color_role::ColorRole::Primary,
        selected_hovered_color: crate::material::tokens::color_role::ColorRole::Primary,
        selected_color: crate::material::tokens::color_role::ColorRole::Primary,
        selected_pressed_color: crate::material::tokens::color_role::ColorRole::Primary,
        unselected_focused_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_hovered_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_pressed_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
    }
}
