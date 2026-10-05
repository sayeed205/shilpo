use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::units::{Dp, Opacity};

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub disabled_selected_icon_color: ColorRole,
    pub disabled_selected_icon_opacity: Opacity,
    pub disabled_unselected_icon_color: ColorRole,
    pub disabled_unselected_icon_opacity: Opacity,
    pub icon_size: Dp,
    pub selected_focus_icon_color: ColorRole,
    pub selected_hover_icon_color: ColorRole,
    pub selected_icon_color: ColorRole,
    pub selected_pressed_icon_color: ColorRole,
    pub state_layer_size: Dp,
    pub unselected_focus_icon_color: ColorRole,
    pub unselected_hover_icon_color: ColorRole,
    pub unselected_icon_color: ColorRole,
    pub unselected_pressed_icon_color: ColorRole,
}

/// Returns authored token values for this component.
pub const fn radio_button() -> Tokens {
    Tokens {
        disabled_selected_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        disabled_selected_icon_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3EC28F5C,
        )),
        disabled_unselected_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        disabled_unselected_icon_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3EC28F5C,
        )),
        icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41A00000)),
        selected_focus_icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        selected_hover_icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        selected_icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        selected_pressed_icon_color: crate::material::tokens::color_role::ColorRole::Primary,
        state_layer_size: crate::material::tokens::units::Dp(f32::from_bits(0x42200000)),
        unselected_focus_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        unselected_hover_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        unselected_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        unselected_pressed_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
    }
}
