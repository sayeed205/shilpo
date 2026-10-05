use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::units::Opacity;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub disabled_container_opacity: Opacity,
    pub disabled_icon_color: ColorRole,
    pub disabled_icon_opacity: Opacity,
    pub disabled_label_text_color: ColorRole,
    pub disabled_label_text_opacity: Opacity,
    pub disabled_outline_color: ColorRole,
    pub focused_icon_color: ColorRole,
    pub focused_label_text_color: ColorRole,
    pub focused_outline_color: ColorRole,
    pub hovered_icon_color: ColorRole,
    pub hovered_label_text_color: ColorRole,
    pub hovered_outline_color: ColorRole,
    pub icon_color: ColorRole,
    pub label_text_color: ColorRole,
    pub outline_color: ColorRole,
    pub pressed_icon_color: ColorRole,
    pub pressed_label_text_color: ColorRole,
    pub pressed_outline_color: ColorRole,
    pub selected_container_color: ColorRole,
    pub selected_disabled_container_color: ColorRole,
    pub selected_focused_icon_color: ColorRole,
    pub selected_focused_label_text_color: ColorRole,
    pub selected_hovered_icon_color: ColorRole,
    pub selected_hovered_label_text_color: ColorRole,
    pub selected_icon_color: ColorRole,
    pub selected_label_text_color: ColorRole,
    pub selected_pressed_icon_color: ColorRole,
    pub selected_pressed_label_text_color: ColorRole,
    pub unselected_disabled_outline_color: ColorRole,
    pub unselected_focused_icon_color: ColorRole,
    pub unselected_focused_label_text_color: ColorRole,
    pub unselected_focused_outline_color: ColorRole,
    pub unselected_hovered_icon_color: ColorRole,
    pub unselected_hovered_label_text_color: ColorRole,
    pub unselected_hovered_outline_color: ColorRole,
    pub unselected_icon_color: ColorRole,
    pub unselected_label_text_color: ColorRole,
    pub unselected_pressed_icon_color: ColorRole,
    pub unselected_pressed_label_text_color: ColorRole,
    pub unselected_pressed_outline_color: ColorRole,
}

/// Returns authored token values for this component.
pub const fn outlined_button() -> Tokens {
    Tokens {
        disabled_container_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3DCCCCCD)),
        disabled_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        disabled_icon_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3EC28F5C)),
        disabled_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        disabled_label_text_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3EC28F5C)),
        disabled_outline_color: crate::material::tokens::color_role::ColorRole::OutlineVariant,
        focused_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        focused_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        focused_outline_color: crate::material::tokens::color_role::ColorRole::OutlineVariant,
        hovered_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        hovered_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        hovered_outline_color: crate::material::tokens::color_role::ColorRole::OutlineVariant,
        icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        label_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        outline_color: crate::material::tokens::color_role::ColorRole::OutlineVariant,
        pressed_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        pressed_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        pressed_outline_color: crate::material::tokens::color_role::ColorRole::OutlineVariant,
        selected_container_color: crate::material::tokens::color_role::ColorRole::InverseSurface,
        selected_disabled_container_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        selected_focused_icon_color: crate::material::tokens::color_role::ColorRole::InverseOnSurface,
        selected_focused_label_text_color:
            crate::material::tokens::color_role::ColorRole::InverseOnSurface,
        selected_hovered_icon_color: crate::material::tokens::color_role::ColorRole::InverseOnSurface,
        selected_hovered_label_text_color:
            crate::material::tokens::color_role::ColorRole::InverseOnSurface,
        selected_icon_color: crate::material::tokens::color_role::ColorRole::InverseOnSurface,
        selected_label_text_color: crate::material::tokens::color_role::ColorRole::InverseOnSurface,
        selected_pressed_icon_color: crate::material::tokens::color_role::ColorRole::InverseOnSurface,
        selected_pressed_label_text_color:
            crate::material::tokens::color_role::ColorRole::InverseOnSurface,
        unselected_disabled_outline_color: crate::material::tokens::color_role::ColorRole::OutlineVariant,
        unselected_focused_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_focused_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_focused_outline_color: crate::material::tokens::color_role::ColorRole::OutlineVariant,
        unselected_hovered_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_hovered_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_hovered_outline_color: crate::material::tokens::color_role::ColorRole::OutlineVariant,
        unselected_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_pressed_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_pressed_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_pressed_outline_color: crate::material::tokens::color_role::ColorRole::OutlineVariant,
    }
}
