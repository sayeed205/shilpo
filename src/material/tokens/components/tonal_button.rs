use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::foundation::ElevationRole;
use crate::material::tokens::units::Opacity;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_color: ColorRole,
    pub container_elevation: ElevationRole,
    pub disabled_container_color: ColorRole,
    pub disabled_container_elevation: ElevationRole,
    pub disabled_container_opacity: Opacity,
    pub disabled_icon_color: ColorRole,
    pub disabled_icon_opacity: Opacity,
    pub disabled_label_text_color: ColorRole,
    pub disabled_label_text_opacity: Opacity,
    pub focused_container_elevation: ElevationRole,
    pub focused_icon_color: ColorRole,
    pub focused_label_text_color: ColorRole,
    pub hovered_container_elevation: ElevationRole,
    pub hovered_icon_color: ColorRole,
    pub hovered_label_text_color: ColorRole,
    pub icon_color: ColorRole,
    pub label_text_color: ColorRole,
    pub pressed_container_elevation: ElevationRole,
    pub pressed_icon_color: ColorRole,
    pub pressed_label_text_color: ColorRole,
    pub selected_container_color: ColorRole,
    pub selected_focused_icon_color: ColorRole,
    pub selected_focused_label_text_color: ColorRole,
    pub selected_hovered_icon_color: ColorRole,
    pub selected_hovered_label_text_color: ColorRole,
    pub selected_icon_color: ColorRole,
    pub selected_label_text_color: ColorRole,
    pub selected_pressed_icon_color: ColorRole,
    pub selected_pressed_label_text_color: ColorRole,
    pub unselected_container_color: ColorRole,
    pub unselected_focused_icon_color: ColorRole,
    pub unselected_focused_label_text_color: ColorRole,
    pub unselected_hovered_icon_color: ColorRole,
    pub unselected_hovered_label_text_color: ColorRole,
    pub unselected_icon_color: ColorRole,
    pub unselected_label_text_color: ColorRole,
    pub unselected_pressed_icon_color: ColorRole,
    pub unselected_pressed_label_text_color: ColorRole,
}

/// Returns authored token values for this component.
pub const fn tonal_button() -> Tokens {
    Tokens {
        container_color: crate::material::tokens::color_role::ColorRole::SecondaryContainer,
        container_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        disabled_container_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        disabled_container_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        disabled_container_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3DCCCCCD)),
        disabled_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        disabled_icon_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3EC28F5C)),
        disabled_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        disabled_label_text_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3EC28F5C)),
        focused_container_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        focused_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        focused_label_text_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        hovered_container_elevation: crate::material::tokens::foundation::ElevationRole::Level1,
        hovered_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        hovered_label_text_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        label_text_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        pressed_container_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        pressed_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        pressed_label_text_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_container_color: crate::material::tokens::color_role::ColorRole::Secondary,
        selected_focused_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondary,
        selected_focused_label_text_color: crate::material::tokens::color_role::ColorRole::OnSecondary,
        selected_hovered_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondary,
        selected_hovered_label_text_color: crate::material::tokens::color_role::ColorRole::OnSecondary,
        selected_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondary,
        selected_label_text_color: crate::material::tokens::color_role::ColorRole::OnSecondary,
        selected_pressed_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondary,
        selected_pressed_label_text_color: crate::material::tokens::color_role::ColorRole::OnSecondary,
        unselected_container_color: crate::material::tokens::color_role::ColorRole::SecondaryContainer,
        unselected_focused_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        unselected_focused_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        unselected_hovered_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        unselected_hovered_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        unselected_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        unselected_label_text_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        unselected_pressed_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        unselected_pressed_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
    }
}
