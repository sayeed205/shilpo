use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::shape::ShapeRole;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub item_container_color: ColorRole,
    pub item_drop_zone_color: ColorRole,
    pub item_label_text_color: ColorRole,
    pub item_leading_icon_color: ColorRole,
    pub item_overline_color: ColorRole,
    pub item_shape: ShapeRole,
    pub item_supporting_text_color: ColorRole,
    pub item_trailing_icon_color: ColorRole,
    pub item_trailing_supporting_text_color: ColorRole,
}

/// Returns authored token values for this component.
pub const fn reorder_list() -> Tokens {
    Tokens {
        item_container_color: crate::material::tokens::color_role::ColorRole::TertiaryContainer,
        item_drop_zone_color: crate::material::tokens::color_role::ColorRole::SurfaceContainerLow,
        item_label_text_color: crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_leading_icon_color: crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_overline_color: crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_shape: crate::material::tokens::shape::ShapeRole::CornerLarge,
        item_supporting_text_color: crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_trailing_icon_color: crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
        item_trailing_supporting_text_color:
            crate::material::tokens::color_role::ColorRole::OnTertiaryContainer,
    }
}
