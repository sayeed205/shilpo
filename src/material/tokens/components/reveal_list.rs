use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::shape::ShapeRole;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub item_action_button_icon_icon_color: ColorRole,
    pub item_action_icon_button_container_color: ColorRole,
    pub item_button_icon_icon_color: ColorRole,
    pub item_container_color: ColorRole,
    pub item_container_shape: ShapeRole,
    pub item_icon_button_action_container_shape: ShapeRole,
    pub item_icon_button_container_color: ColorRole,
    pub item_icon_button_container_shape: ShapeRole,
    pub item_segmented_container_shape: ShapeRole,
}

/// Returns authored token values for this component.
pub const fn reveal_list() -> Tokens {
    Tokens {
        item_action_button_icon_icon_color: crate::material::tokens::color_role::ColorRole::OnPrimary,
        item_action_icon_button_container_color: crate::material::tokens::color_role::ColorRole::Primary,
        item_button_icon_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        item_container_color: crate::material::tokens::color_role::ColorRole::Surface,
        item_container_shape: crate::material::tokens::shape::ShapeRole::CornerLarge,
        item_icon_button_action_container_shape: crate::material::tokens::shape::ShapeRole::CornerLarge,
        item_icon_button_container_color:
            crate::material::tokens::color_role::ColorRole::SecondaryContainer,
        item_icon_button_container_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        item_segmented_container_shape: crate::material::tokens::shape::ShapeRole::CornerLarge,
    }
}
