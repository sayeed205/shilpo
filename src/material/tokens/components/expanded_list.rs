use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::shape::ShapeRole;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub collapsed_item_trailing_icon_container_color: ColorRole,
    pub collapsed_item_trailing_icon_icon_color: ColorRole,
    pub container_shape: ShapeRole,
    pub expanded_item_container_color: ColorRole,
    pub expanded_item_segmented_container_color: ColorRole,
    pub expanded_item_trailing_icon_container_color: ColorRole,
    pub expanded_item_trailing_icon_icon_color: ColorRole,
    pub trailing_icon_shape: ShapeRole,
}

/// Returns authored token values for this component.
pub const fn expanded_list() -> Tokens {
    Tokens {
        collapsed_item_trailing_icon_container_color:
            crate::material::tokens::color_role::ColorRole::Surface,
        collapsed_item_trailing_icon_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        container_shape: crate::material::tokens::shape::ShapeRole::CornerLarge,
        expanded_item_container_color: crate::material::tokens::color_role::ColorRole::Surface,
        expanded_item_segmented_container_color: crate::material::tokens::color_role::ColorRole::Surface,
        expanded_item_trailing_icon_container_color:
            crate::material::tokens::color_role::ColorRole::SurfaceContainer,
        expanded_item_trailing_icon_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        trailing_icon_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
    }
}
