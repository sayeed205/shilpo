use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::units::Opacity;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub disabled_color: ColorRole,
    pub disabled_opacity: Opacity,
    pub disabled_outline_color: ColorRole,
    pub focused_color: ColorRole,
    pub hovered_color: ColorRole,
    pub color: ColorRole,
    pub outline_color: ColorRole,
    pub pressed_color: ColorRole,
    pub selected_container_color: ColorRole,
    pub selected_disabled_container_color: ColorRole,
    pub selected_disabled_container_opacity: Opacity,
    pub selected_focused_color: ColorRole,
    pub selected_hovered_color: ColorRole,
    pub selected_color: ColorRole,
    pub selected_pressed_color: ColorRole,
    pub unselected_disabled_outline_color: ColorRole,
    pub unselected_focused_color: ColorRole,
    pub unselected_hovered_color: ColorRole,
    pub unselected_color: ColorRole,
    pub unselected_outline_color: ColorRole,
    pub unselected_pressed_color: ColorRole,
}

/// Returns authored token values for this component.
pub const fn outlined_icon_button() -> Tokens {
    Tokens {
        disabled_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        disabled_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3EC28F5C)),
        disabled_outline_color: crate::material::tokens::color_role::ColorRole::OutlineVariant,
        focused_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        hovered_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        outline_color: crate::material::tokens::color_role::ColorRole::OutlineVariant,
        pressed_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        selected_container_color: crate::material::tokens::color_role::ColorRole::InverseSurface,
        selected_disabled_container_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        selected_disabled_container_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3DCCCCCD,
        )),
        selected_focused_color: crate::material::tokens::color_role::ColorRole::InverseOnSurface,
        selected_hovered_color: crate::material::tokens::color_role::ColorRole::InverseOnSurface,
        selected_color: crate::material::tokens::color_role::ColorRole::InverseOnSurface,
        selected_pressed_color: crate::material::tokens::color_role::ColorRole::InverseOnSurface,
        unselected_disabled_outline_color: crate::material::tokens::color_role::ColorRole::OutlineVariant,
        unselected_focused_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_hovered_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_outline_color: crate::material::tokens::color_role::ColorRole::OutlineVariant,
        unselected_pressed_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
    }
}
