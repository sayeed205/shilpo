use crate::material::tokens::color_role::ColorRole;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub item_active_focused_state_layer: ColorRole,
    pub item_active_hovered_state_layer: ColorRole,
    pub item_active_icon: ColorRole,
    pub item_active_indicator: ColorRole,
    pub item_active_label_text: ColorRole,
    pub item_active_pressed_state_layer: ColorRole,
    pub item_inactive_focused_state_layer: ColorRole,
    pub item_inactive_hovered_state_layer: ColorRole,
    pub item_inactive_icon: ColorRole,
    pub item_inactive_label_text: ColorRole,
    pub item_inactive_pressed_state_layer: ColorRole,
}

/// Returns authored token values for this component.
pub const fn navigation_rail_color() -> Tokens {
    Tokens {
        item_active_focused_state_layer:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        item_active_hovered_state_layer:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        item_active_icon: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        item_active_indicator: crate::material::tokens::color_role::ColorRole::SecondaryContainer,
        item_active_label_text: crate::material::tokens::color_role::ColorRole::Secondary,
        item_active_pressed_state_layer:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        item_inactive_focused_state_layer:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        item_inactive_hovered_state_layer:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        item_inactive_icon: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        item_inactive_label_text: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        item_inactive_pressed_state_layer:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
    }
}
