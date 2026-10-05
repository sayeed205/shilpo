use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::units::Opacity;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub button_disabled_icon_icon_color: ColorRole,
    pub button_icon_icon_color: ColorRole,
    pub button_selected_icon_icon_color: ColorRole,
    pub container_color: ColorRole,
    pub disabled_button_icon_icon_color: ColorRole,
    pub icon_button_container_color: ColorRole,
    pub icon_button_selected_container_color: ColorRole,
    pub item_container_color: ColorRole,
    pub item_disabled_label_text_color: ColorRole,
    pub item_disabled_label_text_opacity: Opacity,
    pub item_disabled_leading_icon_color: ColorRole,
    pub item_disabled_leading_icon_opacity: Opacity,
    pub item_disabled_supporting_text_color: ColorRole,
    pub item_disabled_supporting_text_opacity: Opacity,
    pub item_disabled_trailing_icon_color: ColorRole,
    pub item_disabled_trailing_icon_opacity: Opacity,
    pub item_disabled_trailing_supporting_text_color: ColorRole,
    pub item_disabled_trailing_supporting_text_opacity: Opacity,
    pub item_focused_label_text_color: ColorRole,
    pub item_focused_leading_icon_color: ColorRole,
    pub item_focused_trailing_icon_color: ColorRole,
    pub item_hovered_label_text_color: ColorRole,
    pub item_hovered_leading_icon_color: ColorRole,
    pub item_hovered_trailing_icon_color: ColorRole,
    pub item_label_text_color: ColorRole,
    pub item_leading_icon_color: ColorRole,
    pub item_pressed_label_text_color: ColorRole,
    pub item_pressed_leading_icon_color: ColorRole,
    pub item_pressed_trailing_icon_color: ColorRole,
    pub item_selected_container_color: ColorRole,
    pub item_selected_disabled_container_color: ColorRole,
    pub item_selected_disabled_container_opacity: Opacity,
    pub item_selected_disabled_label_text_color: ColorRole,
    pub item_selected_disabled_label_text_opacity: Opacity,
    pub item_selected_disabled_leading_icon_color: ColorRole,
    pub item_selected_disabled_leading_icon_opacity: Opacity,
    pub item_selected_disabled_trailing_icon_color: ColorRole,
    pub item_selected_disabled_trailing_icon_opacity: Opacity,
    pub item_selected_disabled_trailing_supporting_text_color: ColorRole,
    pub item_selected_focused_label_text_color: ColorRole,
    pub item_selected_focused_leading_icon_color: ColorRole,
    pub item_selected_focused_supporting_text_color: ColorRole,
    pub item_selected_focused_trailing_icon_color: ColorRole,
    pub item_selected_focused_trailing_supporting_text_color: ColorRole,
    pub item_selected_hovered_label_text_color: ColorRole,
    pub item_selected_hovered_leading_icon_color: ColorRole,
    pub item_selected_hovered_supporting_text_color: ColorRole,
    pub item_selected_hovered_trailing_icon_color: ColorRole,
    pub item_selected_hovered_trailing_supporting_text_color: ColorRole,
    pub item_selected_label_text_color: ColorRole,
    pub item_selected_leading_icon_color: ColorRole,
    pub item_selected_pressed_label_text_color: ColorRole,
    pub item_selected_pressed_leading_icon_color: ColorRole,
    pub item_selected_pressed_supporting_text_color: ColorRole,
    pub item_selected_pressed_trailing_icon_color: ColorRole,
    pub item_selected_pressed_trailing_supporting_text_color: ColorRole,
    pub item_selected_supporting_text_color: ColorRole,
    pub item_selected_trailing_icon_color: ColorRole,
    pub item_selected_trailing_supporting_text_color: ColorRole,
    pub item_supporting_text_color: ColorRole,
    pub item_trailing_icon_color: ColorRole,
    pub item_trailing_supporting_text_color: ColorRole,
}

/// Returns authored token values for this component.
pub const fn standard_menu() -> Tokens {
    Tokens {
        button_disabled_icon_icon_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        button_icon_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        button_selected_icon_icon_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        container_color: crate::material::tokens::color_role::ColorRole::SurfaceContainerLow,
        disabled_button_icon_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        icon_button_container_color: crate::material::tokens::color_role::ColorRole::SurfaceContainerLow,
        icon_button_selected_container_color:
            crate::material::tokens::color_role::ColorRole::TertiaryContainer,
        item_container_color: crate::material::tokens::color_role::ColorRole::SurfaceContainerLow,
        item_disabled_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        item_disabled_label_text_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3EC28F5C,
        )),
        item_disabled_leading_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        item_disabled_leading_icon_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3EC28F5C,
        )),
        item_disabled_supporting_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        item_disabled_supporting_text_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3EC28F5C,
        )),
        item_disabled_trailing_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        item_disabled_trailing_icon_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3EC28F5C,
        )),
        item_disabled_trailing_supporting_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurface,
        item_disabled_trailing_supporting_text_opacity: crate::material::tokens::units::Opacity(
            f32::from_bits(0x3EC28F5C),
        ),
        item_focused_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        item_focused_leading_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        item_focused_trailing_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        item_hovered_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        item_hovered_leading_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        item_hovered_trailing_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        item_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        item_leading_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        item_pressed_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        item_pressed_leading_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        item_pressed_trailing_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        item_selected_container_color: crate::material::tokens::color_role::ColorRole::TertiaryContainer,
        item_selected_disabled_container_color:
            crate::material::tokens::color_role::ColorRole::TertiaryContainer,
        item_selected_disabled_container_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3EC28F5C,
        )),
        item_selected_disabled_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_selected_disabled_label_text_opacity: crate::material::tokens::units::Opacity(
            f32::from_bits(0x3EC28F5C),
        ),
        item_selected_disabled_leading_icon_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_selected_disabled_leading_icon_opacity: crate::material::tokens::units::Opacity(
            f32::from_bits(0x3EC28F5C),
        ),
        item_selected_disabled_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_selected_disabled_trailing_icon_opacity: crate::material::tokens::units::Opacity(
            f32::from_bits(0x3EC28F5C),
        ),
        item_selected_disabled_trailing_supporting_text_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_selected_focused_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_selected_focused_leading_icon_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_selected_focused_supporting_text_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_selected_focused_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_selected_focused_trailing_supporting_text_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_selected_hovered_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_selected_hovered_leading_icon_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_selected_hovered_supporting_text_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_selected_hovered_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_selected_hovered_trailing_supporting_text_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_selected_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_selected_leading_icon_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_selected_pressed_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_selected_pressed_leading_icon_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_selected_pressed_supporting_text_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_selected_pressed_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_selected_pressed_trailing_supporting_text_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_selected_supporting_text_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_selected_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_selected_trailing_supporting_text_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_supporting_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        item_trailing_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        item_trailing_supporting_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
    }
}
